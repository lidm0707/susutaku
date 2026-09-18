//! Postgres adapter for the `task_cards` table + card repo ports.

use async_trait::async_trait;
use task_rs::{
    AddCard, AgentState, CardRow, MoveCard, RunRecordNew, RunRecordRow, StoreError, UpdateCard,
};

use super::{DbTx, PgTask, Store, commit_tx, rollback_tx};
use crate::domain::{CardMove, CardPatch, NewCard};
use crate::port::outbound::{CardRepo, CardTx};

const JSON_ARRAY_ERR: &str = "must be a JSON array";

fn validate_card_json(labels: Option<&str>, checklist: Option<&str>) -> Result<(), StoreError> {
    for (name, value) in [("labels", labels), ("checklist", checklist)] {
        let Some(text) = value else { continue };
        if text.is_empty() {
            continue;
        }
        let parsed: serde_json::Value = serde_json::from_str(text)
            .map_err(|_| StoreError::BadSpec(format!("{name}: invalid JSON")))?;
        if !parsed.is_array() {
            return Err(StoreError::BadSpec(format!("{name}: {JSON_ARRAY_ERR}")));
        }
    }
    Ok(())
}

#[derive(Debug)]
struct NewId {
    id: i64,
}

#[derive(Debug)]
struct AgentRow {
    agent_name: Option<String>,
    agent_state: Option<String>,
}

impl Store {
    /// Cards whose run is queued or running — the in-flight set for the
    /// whole board, across all projects.
    pub async fn running_cards(&self) -> Result<Vec<CardRow>, StoreError> {
        let rows = sqlx::query_as!(
            CardRow,
            r#"SELECT id, column_id, project_id, title, description,
                      priority, position, agent_name, agent_state, run_status, last_agent, last_run_id,
                      assignee, cron, deadline,
                      labels, checklist, estimate, image
               FROM task_cards
               WHERE run_status IN ($1, $2)
               ORDER BY column_id, position, id"#,
            task_rs::RUN_STATUS_QUEUED,
            task_rs::RUN_STATUS_RUNNING
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn list(&self, project_id: Option<i64>) -> Result<Vec<CardRow>, StoreError> {
        let rows = sqlx::query_as!(
            CardRow,
            r#"SELECT id, column_id, project_id, title, description,
                      priority, position, agent_name, agent_state, run_status, last_agent, last_run_id,
                      assignee, cron, deadline,
                      labels, checklist, estimate, image
               FROM task_cards
               WHERE ($1::bigint IS NULL OR project_id = $1)
               ORDER BY column_id, position, id"#,
            project_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn add(&self, card: AddCard<'_>) -> Result<i64, StoreError> {
        validate_card_json(card.labels, card.checklist)?;
        let row = sqlx::query_as!(
            NewId,
            r#"INSERT INTO task_cards (column_id, project_id, title, description, priority, position, labels, checklist, estimate)
               VALUES ($1, $5, $2, $3, $4,
                       COALESCE((SELECT MAX(position) + 1 FROM task_cards WHERE column_id = $1), 0),
                       NULLIF($6, ''), NULLIF($7, ''), $8)
               RETURNING id AS "id: i64""#,
            card.column_id,
            card.title,
            card.description,
            card.priority,
            card.project_id,
            card.labels,
            card.checklist,
            card.estimate,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.id)
    }

    pub async fn get(&self, id: i64) -> Result<Option<CardRow>, StoreError> {
        let row = sqlx::query_as!(
            CardRow,
            r#"SELECT id, column_id, project_id, title, description,
                      priority, position, agent_name, agent_state, run_status, last_agent, last_run_id,
                      assignee, cron, deadline,
                      labels, checklist, estimate, image
               FROM task_cards WHERE id = $1"#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn move_card(&self, mv: MoveCard<'_>) -> Result<(), StoreError> {
        let mut tx = self.begin().await?;
        self.move_card_tx(&mut tx, &mv).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn remove(&self, id: i64) -> Result<(), StoreError> {
        let res = sqlx::query!(r#"DELETE FROM task_cards WHERE id = $1"#, id)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchCard);
        }
        Ok(())
    }

    pub async fn set_agent(&self, id: i64, agent: &AgentState) -> Result<(), StoreError> {
        let state_json = serde_json::to_string(&agent.state)
            .map_err(|e| sqlx::Error::Configuration(e.into()))?;
        let res = sqlx::query!(
            r#"UPDATE task_cards
               SET agent_name = $2, agent_state = $3
               WHERE id = $1"#,
            id,
            agent.name,
            state_json,
        )
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchCard);
        }
        Ok(())
    }

    pub async fn agent(&self, id: i64) -> Result<Option<AgentState>, StoreError> {
        let row = sqlx::query_as!(
            AgentRow,
            r#"SELECT agent_name, agent_state FROM task_cards WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NoSuchCard)?;
        let (name, state) = match (row.agent_name, row.agent_state) {
            (Some(n), Some(s)) => (n, s),
            // A run history without a pinned agent is still readable
            // (pinned agent is a preference; the state is a run ledger).
            (None, Some(s)) => (String::new(), s),
            _ => return Ok(None),
        };
        let state = serde_json::from_str(&state).unwrap_or(serde_json::Value::Null);
        Ok(Some(AgentState { name, state }))
    }

    /// Write only the run ledger (`agent_state`); the pinned agent
    /// (`agent_name`) is a preference and stays untouched.
    pub async fn set_agent_state(&self, id: i64, state_json: &str) -> Result<(), StoreError> {
        let res = sqlx::query!(
            r#"UPDATE task_cards SET agent_state = $2 WHERE id = $1"#,
            id,
            state_json,
        )
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchCard);
        }
        Ok(())
    }

    /// Per-card image (URL or data URI); None clears it.
    pub async fn set_card_image(&self, id: i64, image: Option<&str>) -> Result<(), StoreError> {
        let res = sqlx::query!(
            r#"UPDATE task_cards SET image = $2 WHERE id = $1"#,
            id,
            image,
        )
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchCard);
        }
        Ok(())
    }

    pub async fn set_cron(&self, card_id: i64, cron: Option<&str>) -> Result<(), StoreError> {
        let res = sqlx::query!(
            r#"UPDATE task_cards SET cron = $2 WHERE id = $1"#,
            card_id,
            cron,
        )
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchCard);
        }
        Ok(())
    }

    pub async fn update_card(&self, u: UpdateCard<'_>) -> Result<(), StoreError> {
        validate_card_json(u.labels, u.checklist)?;
        let res = sqlx::query!(
            r#"UPDATE task_cards
               SET title = $2, description = $3, assignee = $4,
                   deadline = NULLIF($5, ''), priority = COALESCE($6, priority),
                   labels = CASE WHEN $7::text IS NULL THEN labels WHEN $7::text = '' THEN NULL ELSE $7::text END,
                   checklist = CASE WHEN $8::text IS NULL THEN checklist WHEN $8::text = '' THEN NULL ELSE $8::text END,
                   estimate = COALESCE($9::int, estimate)
               WHERE id = $1"#,
            u.id,
            u.title,
            u.description,
            u.assignee,
            u.deadline,
            u.priority,
            u.labels,
            u.checklist,
            u.estimate,
        )
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchCard);
        }
        Ok(())
    }

    pub async fn add_tx(&self, tx: &mut DbTx, card: AddCard<'_>) -> Result<i64, StoreError> {
        validate_card_json(card.labels, card.checklist)?;
        let row = sqlx::query_as!(
            NewId,
            r#"INSERT INTO task_cards (column_id, project_id, title, description, priority, position, labels, checklist, estimate)
               VALUES ($1, $5, $2, $3, $4,
                       COALESCE((SELECT MAX(position) + 1 FROM task_cards WHERE column_id = $1), 0),
                       NULLIF($6, ''), NULLIF($7, ''), $8)
               RETURNING id AS "id: i64""#,
            card.column_id,
            card.title,
            card.description,
            card.priority,
            card.project_id,
            card.labels,
            card.checklist,
            card.estimate,
        )
        .fetch_one(&mut **tx)
        .await?;
        Ok(row.id)
    }

    pub async fn get_tx(&self, tx: &mut DbTx, id: i64) -> Result<Option<CardRow>, StoreError> {
        let row = sqlx::query_as!(
            CardRow,
            r#"SELECT id, column_id, project_id, title, description,
                      priority, position, agent_name, agent_state, run_status, last_agent, last_run_id,
                      assignee, cron, deadline,
                      labels, checklist, estimate, image
               FROM task_cards WHERE id = $1"#,
            id
        )
        .fetch_optional(&mut **tx)
        .await?;
        Ok(row)
    }

    pub async fn update_card_tx(&self, tx: &mut DbTx, u: UpdateCard<'_>) -> Result<(), StoreError> {
        validate_card_json(u.labels, u.checklist)?;
        let res = sqlx::query!(
            r#"UPDATE task_cards
               SET title = $2, description = $3, assignee = $4,
                   deadline = NULLIF($5, ''), priority = COALESCE($6, priority),
                   labels = CASE WHEN $7::text IS NULL THEN labels WHEN $7::text = '' THEN NULL ELSE $7::text END,
                   checklist = CASE WHEN $8::text IS NULL THEN checklist WHEN $8::text = '' THEN NULL ELSE $8::text END,
                   estimate = COALESCE($9::int, estimate)
               WHERE id = $1"#,
            u.id,
            u.title,
            u.description,
            u.assignee,
            u.deadline,
            u.priority,
            u.labels,
            u.checklist,
            u.estimate,
        )
        .execute(&mut **tx)
        .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchCard);
        }
        Ok(())
    }

    pub async fn card_exists_tx(&self, tx: &mut DbTx, id: i64) -> Result<bool, StoreError> {
        let found = sqlx::query!(r#"SELECT 1 AS "one!" FROM task_cards WHERE id = $1"#, id)
            .fetch_optional(&mut **tx)
            .await?;
        Ok(found.is_some())
    }

    pub async fn move_card_tx(&self, tx: &mut DbTx, mv: &MoveCard<'_>) -> Result<(), StoreError> {
        let found = sqlx::query!(r#"SELECT 1 AS "one!" FROM task_cards WHERE id = $1"#, mv.id)
            .fetch_optional(&mut **tx)
            .await?;
        if found.is_none() {
            return Err(StoreError::NoSuchCard);
        }
        sqlx::query!(
            r#"UPDATE task_cards
               SET column_id = $2, position = $3
               WHERE id = $1"#,
            mv.id,
            mv.column_id,
            mv.position,
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}

fn as_add(card: &NewCard) -> AddCard<'_> {
    AddCard {
        project_id: card.project_id,
        column_id: &card.column_id,
        title: &card.title,
        description: &card.description,
        priority: &card.priority,
        labels: card.labels.as_deref(),
        checklist: card.checklist.as_deref(),
        estimate: card.estimate,
    }
}

fn as_move(mv: &CardMove) -> MoveCard<'_> {
    MoveCard {
        id: mv.id,
        column_id: &mv.column_id,
        position: mv.position,
    }
}

fn as_patch(patch: &CardPatch) -> UpdateCard<'_> {
    UpdateCard {
        id: patch.id,
        title: &patch.title,
        description: &patch.description,
        assignee: patch.assignee.as_deref(),
        deadline: patch.deadline.as_deref(),
        priority: patch.priority.as_deref(),
        labels: patch.labels.as_deref(),
        checklist: patch.checklist.as_deref(),
        estimate: patch.estimate,
    }
}

#[async_trait]
impl CardRepo for PgTask {
    async fn list(&self, project_id: Option<i64>) -> Result<Vec<CardRow>, StoreError> {
        self.store().list(project_id).await
    }

    async fn get(&self, id: i64) -> Result<Option<CardRow>, StoreError> {
        self.store().get(id).await
    }

    async fn move_card(&self, mv: CardMove) -> Result<(), StoreError> {
        self.store().move_card(as_move(&mv)).await
    }

    async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.store().remove(id).await
    }

    async fn set_agent(&self, id: i64, agent: &AgentState) -> Result<(), StoreError> {
        self.store().set_agent(id, agent).await
    }

    async fn set_agent_state(&self, id: i64, state_json: &str) -> Result<(), StoreError> {
        self.store().set_agent_state(id, state_json).await
    }

    async fn agent(&self, id: i64) -> Result<Option<AgentState>, StoreError> {
        self.store().agent(id).await
    }

    async fn record_run(&self, r: RunRecordNew) -> Result<i64, StoreError> {
        self.store().record_run(r).await
    }

    async fn set_run_start(&self, card_id: i64, agent: &str) -> Result<(), StoreError> {
        self.store().set_run_start(card_id, agent).await
    }

    async fn running_cards(&self) -> Result<Vec<CardRow>, StoreError> {
        self.store().running_cards().await
    }

    async fn card_runs(&self, card_id: i64) -> Result<Vec<RunRecordRow>, StoreError> {
        self.store().card_runs(card_id).await
    }

    async fn set_cron(&self, card_id: i64, cron: Option<String>) -> Result<(), StoreError> {
        self.store().set_cron(card_id, cron.as_deref()).await
    }

    async fn set_card_image<'a>(&self, id: i64, image: Option<&'a str>) -> Result<(), StoreError> {
        self.store().set_card_image(id, image).await
    }

    async fn tx(&self) -> Result<Box<dyn CardTx>, StoreError> {
        Ok(Box::new(PgCardTx {
            store: std::sync::Arc::clone(&self.store),
            tx: self.store().begin().await?,
        }))
    }
}

struct PgCardTx {
    store: std::sync::Arc<Store>,
    tx: DbTx,
}

#[async_trait]
impl CardTx for PgCardTx {
    async fn add(&mut self, card: NewCard) -> Result<i64, StoreError> {
        self.store.add_tx(&mut self.tx, as_add(&card)).await
    }

    async fn get(&mut self, id: i64) -> Result<Option<CardRow>, StoreError> {
        self.store.get_tx(&mut self.tx, id).await
    }

    async fn update(&mut self, patch: CardPatch) -> Result<(), StoreError> {
        self.store
            .update_card_tx(&mut self.tx, as_patch(&patch))
            .await
    }

    async fn commit(self: Box<Self>) -> Result<(), StoreError> {
        let PgCardTx { tx, .. } = *self;
        commit_tx(tx).await
    }

    async fn rollback(self: Box<Self>) -> Result<(), StoreError> {
        let PgCardTx { tx, .. } = *self;
        rollback_tx(tx).await
    }
}
