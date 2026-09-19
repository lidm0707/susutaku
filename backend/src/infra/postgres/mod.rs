//! Infrastructure: Postgres-backed adapters and stores.
//! One file per table (group): `workspace` = workspaces+projects,
//! `card` = task_cards, `run` = run_records, `activity`, `comment`,
//! `resource`, `agent`, `skill`, `chat`, `user` = users+auth_sessions,
//! `agent_token` = agent_run_tokens, `routine` = routines+routine_runs,
//! `attachment` = card_attachments, `codex_usage`.

pub mod activity;
pub mod agent;
pub mod agent_token;
pub mod attachment;
pub mod card;
pub mod chat;
pub mod codex_usage;
pub mod comment;
pub mod resource;
pub mod routine;
pub mod run;
pub mod skill;
pub mod user;
pub mod workspace;

use sqlx::PgPool;

use task_rs::StoreError;

pub const DATABASE_URL_ENV: &str = "DATABASE_URL";

pub const DEFAULT_DATABASE_URL: &str = "postgres://susutaku:susutaku@localhost:5434/susutaku";

const CONNECT_RETRIES: usize = 5;
const CONNECT_RETRY_DELAY_SECS: u64 = 2;

/// Open transaction on the store pool; repos run multi-statement writes on it.
pub type DbTx = sqlx::Transaction<'static, sqlx::Postgres>;

/// Postgres handle: every table query in this module tree runs through it.
#[derive(Clone)]
pub struct Store {
    pub(crate) pool: PgPool,
}

const MIGRATION_SQL: &[&str] = &[
    // legacy kanban_* table names -> task_* (no-op on fresh databases)
    r#"
ALTER TABLE IF EXISTS kanban_cards RENAME TO task_cards;
"#,
    r#"
ALTER TABLE IF EXISTS kanban_comments RENAME TO task_comments;
"#,
    r#"
ALTER INDEX IF EXISTS kanban_cards_column_idx RENAME TO task_cards_column_idx;
"#,
    r#"
ALTER INDEX IF EXISTS kanban_comments_card_idx RENAME TO task_comments_card_idx;
"#,
    r#"
CREATE TABLE IF NOT EXISTS task_cards (
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
CREATE INDEX IF NOT EXISTS task_cards_column_idx ON task_cards (column_id, position);
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
ALTER TABLE task_cards ADD COLUMN IF NOT EXISTS project_id BIGINT REFERENCES projects(id) ON DELETE CASCADE;
"#,
    r#"
ALTER TABLE task_cards DROP COLUMN IF EXISTS pipeline_id;
"#,
    r#"
DROP TABLE IF EXISTS pipelines;
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
ALTER TABLE agent_settings ADD COLUMN IF NOT EXISTS allowed_tools TEXT[] NOT NULL DEFAULT '{}';
"#,
    r#"
ALTER TABLE agent_settings ADD COLUMN IF NOT EXISTS receive_images BOOLEAN NOT NULL DEFAULT TRUE;
"#,
    r#"
ALTER TABLE agent_settings ADD COLUMN IF NOT EXISTS thinking TEXT NOT NULL DEFAULT 'off';
"#,
    r#"
ALTER TABLE agent_settings ADD COLUMN IF NOT EXISTS ctx_limit BIGINT NOT NULL DEFAULT 128000;
"#,
    r#"
ALTER TABLE agent_settings ADD COLUMN IF NOT EXISTS ctx_policy TEXT NOT NULL DEFAULT 'compact';
"#,
    r#"
ALTER TABLE agent_settings ALTER COLUMN ctx_policy SET DEFAULT 'compact';
"#,
    r#"
CREATE TABLE IF NOT EXISTS skills (
    id   BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    body TEXT NOT NULL DEFAULT ''
);
"#,
    r#"
CREATE TABLE IF NOT EXISTS agent_skills (
    agent_id BIGINT NOT NULL REFERENCES agent_settings(id) ON DELETE CASCADE,
    skill_id BIGINT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
    PRIMARY KEY (agent_id, skill_id)
);
"#,
    r#"
ALTER TABLE task_cards ADD COLUMN IF NOT EXISTS assignee TEXT;
"#,
    r#"
CREATE TABLE IF NOT EXISTS task_comments (
    id         BIGSERIAL PRIMARY KEY,
    card_id    BIGINT NOT NULL REFERENCES task_cards(id) ON DELETE CASCADE,
    author     TEXT NOT NULL,
    body       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#,
    r#"
CREATE INDEX IF NOT EXISTS task_comments_card_idx ON task_comments (card_id, id);
"#,
    r#"
ALTER TABLE task_cards ADD COLUMN IF NOT EXISTS cron TEXT;
"#,
    r#"
ALTER TABLE task_cards ADD COLUMN IF NOT EXISTS deadline TEXT;
"#,
    r#"
ALTER TABLE task_cards ADD COLUMN IF NOT EXISTS labels TEXT;
"#,
    r#"
ALTER TABLE task_cards ADD COLUMN IF NOT EXISTS checklist TEXT;
"#,
    r#"
ALTER TABLE task_cards ADD COLUMN IF NOT EXISTS estimate INT;
"#,
    r#"
ALTER TABLE task_cards ADD COLUMN IF NOT EXISTS image TEXT;
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
    r#"
CREATE TABLE IF NOT EXISTS card_resources (
    id         BIGSERIAL PRIMARY KEY,
    card_id    BIGINT NOT NULL REFERENCES task_cards(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    content    TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (card_id, name)
);
"#,
    r#"
CREATE INDEX IF NOT EXISTS card_resources_card_idx ON card_resources (card_id, id);
"#,
    r#"
DO $$
BEGIN
    IF to_regclass('pipeline_resources') IS NOT NULL THEN
        INSERT INTO card_resources (card_id, name, content, created_at)
        SELECT card_id, name, content, created_at FROM pipeline_resources
        ON CONFLICT (card_id, name) DO NOTHING;
        DROP TABLE pipeline_resources;
    END IF;
END
$$;
"#,
    r#"
DROP TABLE IF EXISTS agent_outputs;
"#,
    r#"
DROP INDEX IF EXISTS agent_outputs_status_idx;
"#,
    r#"
CREATE TABLE IF NOT EXISTS chat_threads (
    id         BIGSERIAL PRIMARY KEY,
    agent      TEXT NOT NULL,
    title      TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#,
    r#"
CREATE TABLE IF NOT EXISTS chat_messages (
    id         BIGSERIAL PRIMARY KEY,
    thread_id  BIGINT NOT NULL REFERENCES chat_threads(id) ON DELETE CASCADE,
    role       TEXT NOT NULL,
    text       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#,
    r#"
CREATE INDEX IF NOT EXISTS chat_messages_thread_idx ON chat_messages (thread_id, id);
"#,
    r#"
ALTER TABLE task_cards ADD COLUMN IF NOT EXISTS run_status TEXT NOT NULL DEFAULT 'idle';
"#,
    r#"
ALTER TABLE task_cards ADD COLUMN IF NOT EXISTS last_agent TEXT;
"#,
    r#"
ALTER TABLE task_cards ADD COLUMN IF NOT EXISTS last_run_id BIGINT;
"#,
    r#"
CREATE TABLE IF NOT EXISTS run_records (
    id          BIGSERIAL PRIMARY KEY,
    card_id     BIGINT NOT NULL REFERENCES task_cards(id) ON DELETE CASCADE,
    "trigger"   TEXT NOT NULL,
    agent       TEXT NOT NULL DEFAULT '',
    started_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,
    ok          BOOLEAN NOT NULL DEFAULT FALSE,
    summary     TEXT NOT NULL DEFAULT ''
);
"#,
    r#"
CREATE INDEX IF NOT EXISTS run_records_card_idx ON run_records (card_id, id DESC);
"#,
    r#"
ALTER TABLE chat_threads ADD COLUMN IF NOT EXISTS project_id BIGINT REFERENCES projects(id) ON DELETE CASCADE;
"#,
    r#"
CREATE TABLE IF NOT EXISTS agent_run_tokens (
    token_hash TEXT PRIMARY KEY,
    agent      TEXT NOT NULL,
    project_id BIGINT REFERENCES projects(id) ON DELETE CASCADE,
    card_id    BIGINT REFERENCES task_cards(id) ON DELETE CASCADE,
    thread_id  BIGINT REFERENCES chat_threads(id) ON DELETE CASCADE,
    machine    TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);
"#,
    r#"
CREATE TABLE IF NOT EXISTS routines (
    id          BIGSERIAL PRIMARY KEY,
    name        TEXT NOT NULL,
    cron        TEXT NOT NULL,
    agent       TEXT NOT NULL DEFAULT '',
    instruction TEXT NOT NULL DEFAULT '',
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#,
    r#"
CREATE TABLE IF NOT EXISTS routine_runs (
    id          BIGSERIAL PRIMARY KEY,
    routine_id  BIGINT NOT NULL REFERENCES routines(id) ON DELETE CASCADE,
    trigger     TEXT NOT NULL DEFAULT 'cron',
    started_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,
    ok          BOOLEAN NOT NULL DEFAULT FALSE,
    summary     TEXT NOT NULL DEFAULT ''
);
"#,
    r#"
CREATE INDEX IF NOT EXISTS routine_runs_routine_idx ON routine_runs (routine_id, id DESC);
"#,
    r#"
CREATE TABLE IF NOT EXISTS card_attachments (
    id         BIGSERIAL PRIMARY KEY,
    card_id    BIGINT NOT NULL REFERENCES task_cards(id) ON DELETE CASCADE,
    path       TEXT NOT NULL,
    name       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (card_id, path)
);
"#,
    r#"
CREATE INDEX IF NOT EXISTS card_attachments_path_idx ON card_attachments (path);
"#,
    r#"
CREATE INDEX IF NOT EXISTS task_cards_project_idx ON task_cards (project_id);
"#,
    r#"
CREATE INDEX IF NOT EXISTS chat_threads_project_idx ON chat_threads (project_id);
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

    pub(crate) async fn begin(&self) -> Result<DbTx, StoreError> {
        let tx = self.pool.begin().await?;
        Ok(tx)
    }
}

pub(crate) async fn commit_tx(tx: DbTx) -> Result<(), StoreError> {
    tx.commit().await.map_err(Into::into)
}

pub(crate) async fn rollback_tx(tx: DbTx) -> Result<(), StoreError> {
    tx.rollback().await.map_err(Into::into)
}

pub async fn connect() -> Store {
    let url = std::env::var(DATABASE_URL_ENV).unwrap_or_else(|_| DEFAULT_DATABASE_URL.into());
    let mut store = None;
    for attempt in 1..=CONNECT_RETRIES {
        match Store::connect(&url).await {
            Ok(s) => {
                store = Some(s);
                break;
            }
            Err(err) => {
                eprintln!(
                    "task postgres connect (attempt {attempt}/{CONNECT_RETRIES}): {err:?} \
                     — is the postgres container up? (docker/compose/base.yml, host port 5434)"
                );
                if attempt < CONNECT_RETRIES {
                    tokio::time::sleep(std::time::Duration::from_secs(CONNECT_RETRY_DELAY_SECS))
                        .await;
                }
            }
        }
    }
    let store = store.expect("task postgres connect: gave up");
    store
        .ensure_default_admin()
        .await
        .expect("seed default admin");
    store
        .ensure_default_board()
        .await
        .expect("seed default workspace/project");
    store
}

/// Postgres-backed implementation of every task repo port. The impls live
/// in the per-table modules (`card`, `comment`, …).
pub struct PgTask {
    pub(crate) store: std::sync::Arc<Store>,
}

impl PgTask {
    pub fn new(store: std::sync::Arc<Store>) -> Self {
        Self { store }
    }

    pub(crate) fn store(&self) -> &Store {
        &self.store
    }
}
