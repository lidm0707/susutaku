//! Domain service bound 1:1 to the CardRepo port.

use std::sync::Arc;

use task_rs::{
    AgentState, CardRow, RunRecordNew, RunRecordRow, StoreError, TaskStatus, transition_allowed,
};

use crate::domain::{CardMove, CardPatch, NewCard};
use crate::port::outbound::CardRepo;

pub struct CardService {
    repo: Arc<dyn CardRepo>,
}

pub const MOVE_STATUS_TOP: i32 = 0;

impl CardService {
    pub fn new(repo: Arc<dyn CardRepo>) -> Self {
        Self { repo }
    }

    pub async fn list(&self, project_id: Option<i64>) -> Result<Vec<CardRow>, StoreError> {
        self.repo.list(project_id).await
    }

    pub async fn get(&self, id: i64) -> Result<Option<CardRow>, StoreError> {
        self.repo.get(id).await
    }

    /// Write + read-back inside one transaction.
    pub async fn create(&self, card: NewCard) -> Result<CardRow, StoreError> {
        let mut tx = self.repo.tx().await?;
        let id = tx.add(card).await?;
        let row = tx.get(id).await?.ok_or(StoreError::NoSuchCard)?;
        tx.commit().await?;
        Ok(row)
    }

    /// Edit + read-back inside one transaction.
    pub async fn update(&self, patch: CardPatch) -> Result<CardRow, StoreError> {
        let id = patch.id;
        let mut tx = self.repo.tx().await?;
        tx.update(patch).await?;
        let row = tx.get(id).await?.ok_or(StoreError::NoSuchCard)?;
        tx.commit().await?;
        Ok(row)
    }

    pub async fn move_card(&self, mv: CardMove) -> Result<(), StoreError> {
        self.repo.move_card(mv).await
    }

    /// Move a task to a new workflow status, validating the transition
    /// server-side. Reuses move/get — no new SQL.
    pub async fn set_status(&self, id: i64, to: TaskStatus) -> Result<CardRow, StoreError> {
        let row = self.repo.get(id).await?.ok_or(StoreError::NoSuchCard)?;
        let from = TaskStatus::parse(&row.column_id);
        if from != to {
            if !transition_allowed(from, to) {
                return Err(StoreError::BadSpec(format!(
                    "illegal task status transition {} -> {}",
                    from.as_str(),
                    to.as_str()
                )));
            }
            self.repo
                .move_card(CardMove {
                    id,
                    column_id: to.column().to_owned(),
                    position: MOVE_STATUS_TOP,
                })
                .await?;
        }
        self.repo.get(id).await?.ok_or(StoreError::NoSuchCard)
    }

    pub async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.repo.remove(id).await
    }

    pub async fn set_agent(&self, id: i64, agent: &AgentState) -> Result<(), StoreError> {
        self.repo.set_agent(id, agent).await
    }

    pub async fn set_agent_state(&self, id: i64, state_json: &str) -> Result<(), StoreError> {
        self.repo.set_agent_state(id, state_json).await
    }

    pub async fn agent(&self, id: i64) -> Result<Option<AgentState>, StoreError> {
        self.repo.agent(id).await
    }

    pub async fn record_run(&self, r: RunRecordNew) -> Result<i64, StoreError> {
        self.repo.record_run(r).await
    }

    pub async fn set_run_start(&self, card_id: i64, agent: &str) -> Result<(), StoreError> {
        self.repo.set_run_start(card_id, agent).await
    }

    pub async fn card_runs(&self, card_id: i64) -> Result<Vec<RunRecordRow>, StoreError> {
        self.repo.card_runs(card_id).await
    }

    pub async fn set_cron(&self, card_id: i64, cron: Option<String>) -> Result<(), StoreError> {
        self.repo.set_cron(card_id, cron).await
    }

    pub async fn set_card_image(
        &self,
        card_id: i64,
        image: Option<&str>,
    ) -> Result<(), StoreError> {
        self.repo.set_card_image(card_id, image).await
    }
}
