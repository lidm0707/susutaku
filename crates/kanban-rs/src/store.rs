//! Postgres persistence: cards + per-card agent state, sqlx `query_as!` only.

use sqlx::PgPool;

pub const DEFAULT_DATABASE_URL: &str =
    "postgres://susutaku:susutaku@localhost:5434/susutaku";

pub const TABLE_NAME: &str = "kanban_cards";

pub const NO_SUCH_CARD_MSG: &str = "no such card";
pub const NO_SUCH_COLUMN_MSG: &str = "no such column";

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
    pub pipeline_id: Option<i64>,
    pub pipeline_name: Option<String>,
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
}

pub struct MoveCard<'a> {
    pub id: i64,
    pub column_id: &'a str,
    pub position: i32,
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
];

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
            r#"SELECT c.id, c.column_id, c.project_id, c.title, c.description,
                      c.priority, c.position, c.agent_name, c.agent_state,
                      c.pipeline_id, p.name AS "pipeline_name: Option<String>"
               FROM kanban_cards c
               LEFT JOIN pipelines p ON p.id = c.pipeline_id
               WHERE ($1::bigint IS NULL OR c.project_id = $1)
               ORDER BY c.column_id, c.position, c.id"#,
            project_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn add(&self, card: AddCard<'_>) -> Result<i64, StoreError> {
        let row = sqlx::query_as!(
            NewId,
            r#"INSERT INTO kanban_cards (column_id, project_id, title, description, priority, position)
               VALUES ($1, $5, $2, $3, $4,
                       COALESCE((SELECT MAX(position) + 1 FROM kanban_cards WHERE column_id = $1), 0))
               RETURNING id AS "id: i64""#,
            card.column_id,
            card.title,
            card.description,
            card.priority,
            card.project_id,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.id)
    }

    pub async fn get(&self, id: i64) -> Result<Option<CardRow>, StoreError> {
        let row = sqlx::query_as!(
            CardRow,
            r#"SELECT c.id, c.column_id, c.project_id, c.title, c.description,
                      c.priority, c.position, c.agent_name, c.agent_state,
                      c.pipeline_id, p.name AS "pipeline_name: Option<String>"
               FROM kanban_cards c
               LEFT JOIN pipelines p ON p.id = c.pipeline_id
               WHERE c.id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn move_card(&self, mv: MoveCard<'_>) -> Result<(), StoreError> {
        let found = sqlx::query!(
            r#"SELECT 1 AS "one!" FROM kanban_cards WHERE id = $1"#,
            mv.id
        )
        .fetch_optional(&self.pool)
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
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove(&self, id: i64) -> Result<(), StoreError> {
        let res = sqlx::query!(
            r#"DELETE FROM kanban_cards WHERE id = $1"#,
            id
        )
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
