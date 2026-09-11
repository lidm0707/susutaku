//! Postgres persistence: cards + per-card agent state, sqlx `query_as!` only.

use sqlx::PgPool;

pub const DEFAULT_DATABASE_URL: &str = "postgres://susutaku:susutaku@localhost:5434/susutaku";

pub const TABLE_NAME: &str = "kanban_cards";

pub const NO_SUCH_CARD_MSG: &str = "no such card";
pub const NO_SUCH_COLUMN_MSG: &str = "no such column";

pub const ACTIVITY_MESSAGE_MAX_CHARS: usize = 500;
pub const ACTIVITY_LIST_MAX: i64 = 200;
pub const ACTIVITY_LIST_DEFAULT: i64 = 50;

/// Open transaction on the store pool; repos run multi-statement writes on it.
pub type DbTx = sqlx::Transaction<'static, sqlx::Postgres>;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{NO_SUCH_CARD_MSG}")]
    NoSuchCard,
    #[error("{NO_SUCH_COLUMN_MSG}")]
    NoSuchColumn,
    #[error("no such workspace")]
    NoSuchWorkspace,
    #[error("no such project")]
    NoSuchProject,
    #[error("workspace name already taken")]
    WorkspaceTaken,
    #[error("project name already taken")]
    ProjectTaken,
    #[error("username already taken")]
    UsernameTaken,
    #[error("pipeline name already taken")]
    PipelineTaken,
    #[error("no such pipeline")]
    NoSuchPipeline,
    #[error("agent name already taken")]
    AgentTaken,
    #[error("no such agent")]
    NoSuchAgent,
    #[error("bad pipeline spec: {0}")]
    BadSpec(String),
    #[error("password too short")]
    PasswordTooShort,
    #[error("invalid credentials")]
    BadCredentials,
    #[error("unknown role: {0}")]
    BadRole(String),
    #[error("hash error: {0}")]
    Hash(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct CardRow {
    pub id: i64,
    pub column_id: String,
    pub project_id: Option<i64>,
    pub title: String,
    pub description: String,
    pub priority: String,
    pub position: i32,
    pub agent_name: Option<String>,
    pub agent_state: Option<String>,
    pub assignee: Option<String>,
    pub pipeline_id: Option<i64>,
    pub cron: Option<String>,
    pub deadline: Option<String>,
    /// JSON array of label strings; empty string clears, NULL/None unset.
    pub labels: Option<String>,
    /// JSON array of {text, done} objects; empty string clears, NULL/None unset.
    pub checklist: Option<String>,
    pub estimate: Option<i32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentState {
    pub name: String,
    #[serde(default)]
    pub state: serde_json::Value,
}

pub struct AddCard<'a> {
    pub project_id: Option<i64>,
    pub column_id: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub priority: &'a str,
    /// JSON array of label strings; empty string clears, None unset.
    pub labels: Option<&'a str>,
    /// JSON array of {text, done} objects; empty string clears, None unset.
    pub checklist: Option<&'a str>,
    pub estimate: Option<i32>,
}

pub struct MoveCard<'a> {
    pub id: i64,
    pub column_id: &'a str,
    pub position: i32,
}

pub struct UpdateCard<'a> {
    pub id: i64,
    pub title: &'a str,
    pub description: &'a str,
    pub assignee: Option<&'a str>,
    /// Empty string clears the deadline; NULL keeps it unset.
    pub deadline: Option<&'a str>,
    /// None leaves the priority unchanged.
    pub priority: Option<&'a str>,
    /// None leaves unchanged; empty string clears. Must be a JSON array when set.
    pub labels: Option<&'a str>,
    /// None leaves unchanged; empty string clears. Must be a JSON array when set.
    pub checklist: Option<&'a str>,
    /// Story points; None leaves the current value unchanged.
    pub estimate: Option<i32>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ActivityRow {
    pub id: i64,
    pub kind: String,
    pub message: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CommentRow {
    pub id: i64,
    pub card_id: i64,
    pub author: String,
    pub body: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone)]
pub struct Store {
    pub(crate) pool: PgPool,
}

const MIGRATION_SQL: &[&str] = &[
    r#"
CREATE TABLE IF NOT EXISTS kanban_cards (
    id          BIGSERIAL PRIMARY KEY,
    column_id   TEXT NOT NULL,
    title       TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    priority    TEXT NOT NULL DEFAULT 'normal',
    position    INT  NOT NULL DEFAULT 0,
    agent_name  TEXT,
    agent_state TEXT
);
"#,
    r#"
CREATE INDEX IF NOT EXISTS kanban_cards_column_idx ON kanban_cards (column_id, position);
"#,
    r#"
CREATE TABLE IF NOT EXISTS users (
    id            BIGSERIAL PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL DEFAULT 'viewer'
);
"#,
    r#"
ALTER TABLE users ADD COLUMN IF NOT EXISTS must_change_password BOOLEAN NOT NULL DEFAULT FALSE;
"#,
    r#"
CREATE TABLE IF NOT EXISTS auth_sessions (
    token      TEXT PRIMARY KEY,
    user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#,
    r#"
CREATE TABLE IF NOT EXISTS workspaces (
    id   BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);
"#,
    r#"
CREATE TABLE IF NOT EXISTS projects (
    id           BIGSERIAL PRIMARY KEY,
    workspace_id BIGINT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    UNIQUE (workspace_id, name)
);
"#,
    r#"
ALTER TABLE kanban_cards ADD COLUMN IF NOT EXISTS project_id BIGINT REFERENCES projects(id) ON DELETE CASCADE;
"#,
    r#"
CREATE TABLE IF NOT EXISTS pipelines (
    id   BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    spec TEXT NOT NULL
);
"#,
    r#"
CREATE TABLE IF NOT EXISTS agent_settings (
    id      BIGSERIAL PRIMARY KEY,
    name    TEXT NOT NULL UNIQUE,
    model   TEXT NOT NULL DEFAULT '',
    persona TEXT NOT NULL DEFAULT '',
    prompt  TEXT NOT NULL DEFAULT '',
    output  TEXT NOT NULL DEFAULT ''
);
"#,
    r#"
ALTER TABLE kanban_cards ADD COLUMN IF NOT EXISTS pipeline_id BIGINT REFERENCES pipelines(id) ON DELETE SET NULL;
"#,
    r#"
ALTER TABLE kanban_cards ADD COLUMN IF NOT EXISTS assignee TEXT;
"#,
    r#"
CREATE TABLE IF NOT EXISTS kanban_comments (
    id         BIGSERIAL PRIMARY KEY,
    card_id    BIGINT NOT NULL REFERENCES kanban_cards(id) ON DELETE CASCADE,
    author     TEXT NOT NULL,
    body       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#,
    r#"
CREATE INDEX IF NOT EXISTS kanban_comments_card_idx ON kanban_comments (card_id, id);
"#,
    r#"
ALTER TABLE kanban_cards ADD COLUMN IF NOT EXISTS cron TEXT;
"#,
    r#"
ALTER TABLE kanban_cards ADD COLUMN IF NOT EXISTS deadline TEXT;
"#,
    r#"
ALTER TABLE kanban_cards ADD COLUMN IF NOT EXISTS labels TEXT;
"#,
    r#"
ALTER TABLE kanban_cards ADD COLUMN IF NOT EXISTS checklist TEXT;
"#,
    r#"
ALTER TABLE kanban_cards ADD COLUMN IF NOT EXISTS estimate INT;
"#,
    r#"
CREATE TABLE IF NOT EXISTS activity (
    id         BIGSERIAL PRIMARY KEY,
    kind       TEXT NOT NULL,
    message    TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#,
    r#"
CREATE INDEX IF NOT EXISTS activity_created_idx ON activity (created_at DESC, id DESC);
"#,
];

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

impl Store {
    pub fn default_url() -> &'static str {
        DEFAULT_DATABASE_URL
    }

    pub async fn connect(url: &str) -> Result<Self, StoreError> {
        let pool = PgPool::connect(url).await?;
        for stmt in MIGRATION_SQL {
            sqlx::query(stmt).execute(&pool).await?;
        }
        Ok(Self { pool })
    }

    pub async fn list(&self, project_id: Option<i64>) -> Result<Vec<CardRow>, StoreError> {
        let rows = sqlx::query_as!(
            CardRow,
            r#"SELECT id, column_id, project_id, title, description,
                      priority, position, agent_name, agent_state, assignee, pipeline_id, cron, deadline,
                      labels, checklist, estimate
               FROM kanban_cards
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
            r#"INSERT INTO kanban_cards (column_id, project_id, title, description, priority, position, labels, checklist, estimate)
               VALUES ($1, $5, $2, $3, $4,
                       COALESCE((SELECT MAX(position) + 1 FROM kanban_cards WHERE column_id = $1), 0),
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
                      priority, position, agent_name, agent_state, assignee, pipeline_id, cron, deadline,
                      labels, checklist, estimate
               FROM kanban_cards WHERE id = $1"#,
            id
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
        let res = sqlx::query!(r#"DELETE FROM kanban_cards WHERE id = $1"#, id)
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
            r#"UPDATE kanban_cards
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
            r#"SELECT agent_name, agent_state FROM kanban_cards WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NoSuchCard)?;
        let (name, state) = match (row.agent_name, row.agent_state) {
            (Some(n), Some(s)) => (n, s),
            _ => return Ok(None),
        };
        let state = serde_json::from_str(&state).unwrap_or(serde_json::Value::Null);
        Ok(Some(AgentState { name, state }))
    }

    pub async fn set_card_pipeline(
        &self,
        card_id: i64,
        pipeline_id: Option<i64>,
    ) -> Result<(), StoreError> {
        let res = sqlx::query!(
            r#"UPDATE kanban_cards SET pipeline_id = $2 WHERE id = $1"#,
            card_id,
            pipeline_id,
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
            r#"UPDATE kanban_cards SET cron = $2 WHERE id = $1"#,
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
            r#"UPDATE kanban_cards
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

    pub async fn list_comments(&self, card_id: i64) -> Result<Vec<CommentRow>, StoreError> {
        let rows = sqlx::query_as!(
            CommentRow,
            r#"SELECT id, card_id, author, body, created_at
               FROM kanban_comments WHERE card_id = $1 ORDER BY id"#,
            card_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn add_comment(
        &self,
        card_id: i64,
        author: &str,
        body: &str,
    ) -> Result<i64, StoreError> {
        let mut tx = self.begin().await?;
        if !self.card_exists_tx(&mut tx, card_id).await? {
            return Err(StoreError::NoSuchCard);
        }
        let id = self
            .add_comment_tx(&mut tx, card_id, author, body)
            .await?
            .id;
        tx.commit().await?;
        Ok(id)
    }

    fn clamp_message(message: &str) -> String {
        message.chars().take(ACTIVITY_MESSAGE_MAX_CHARS).collect()
    }

    pub async fn record_activity(&self, kind: &str, message: &str) -> Result<(), StoreError> {
        sqlx::query!(
            r#"INSERT INTO activity (kind, message) VALUES ($1, $2)"#,
            kind,
            Self::clamp_message(message),
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_activity(&self, limit: i64) -> Result<Vec<ActivityRow>, StoreError> {
        let limit = limit.clamp(1, ACTIVITY_LIST_MAX);
        let rows = sqlx::query_as!(
            ActivityRow,
            r#"SELECT id, kind, message, created_at FROM activity ORDER BY id DESC LIMIT $1"#,
            limit
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn begin(&self) -> Result<DbTx, StoreError> {
        let tx = self.pool.begin().await?;
        Ok(tx)
    }

    pub async fn add_tx(&self, tx: &mut DbTx, card: AddCard<'_>) -> Result<i64, StoreError> {
        validate_card_json(card.labels, card.checklist)?;
        let row = sqlx::query_as!(
            NewId,
            r#"INSERT INTO kanban_cards (column_id, project_id, title, description, priority, position, labels, checklist, estimate)
               VALUES ($1, $5, $2, $3, $4,
                       COALESCE((SELECT MAX(position) + 1 FROM kanban_cards WHERE column_id = $1), 0),
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
                      priority, position, agent_name, agent_state, assignee, pipeline_id, cron, deadline,
                      labels, checklist, estimate
               FROM kanban_cards WHERE id = $1"#,
            id
        )
        .fetch_optional(&mut **tx)
        .await?;
        Ok(row)
    }

    pub async fn update_card_tx(&self, tx: &mut DbTx, u: UpdateCard<'_>) -> Result<(), StoreError> {
        validate_card_json(u.labels, u.checklist)?;
        let res = sqlx::query!(
            r#"UPDATE kanban_cards
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
        let found = sqlx::query!(r#"SELECT 1 AS "one!" FROM kanban_cards WHERE id = $1"#, id)
            .fetch_optional(&mut **tx)
            .await?;
        Ok(found.is_some())
    }

    /// Inserts a comment and returns the full row (id, author, body, created_at).
    pub async fn add_comment_tx(
        &self,
        tx: &mut DbTx,
        card_id: i64,
        author: &str,
        body: &str,
    ) -> Result<CommentRow, StoreError> {
        let row = sqlx::query_as!(
            CommentRow,
            r#"INSERT INTO kanban_comments (card_id, author, body)
               VALUES ($1, $2, $3)
               RETURNING id, card_id, author, body, created_at"#,
            card_id,
            author,
            body,
        )
        .fetch_one(&mut **tx)
        .await?;
        Ok(row)
    }

    pub async fn move_card_tx(&self, tx: &mut DbTx, mv: &MoveCard<'_>) -> Result<(), StoreError> {
        let found = sqlx::query!(
            r#"SELECT 1 AS "one!" FROM kanban_cards WHERE id = $1"#,
            mv.id
        )
        .fetch_optional(&mut **tx)
        .await?;
        if found.is_none() {
            return Err(StoreError::NoSuchCard);
        }
        sqlx::query!(
            r#"UPDATE kanban_cards
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

#[derive(Debug)]
struct NewId {
    id: i64,
}

#[derive(Debug)]
struct AgentRow {
    agent_name: Option<String>,
    agent_state: Option<String>,
}
