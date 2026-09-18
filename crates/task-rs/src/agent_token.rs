//! Agent run tickets: temporary tokens carrying run routing context.
//! Plaintext token is returned once; only the SHA-256 hash is stored.
//! Pure model — the SQL lives in the backend's postgres adapter.

use serde::{Deserialize, Serialize};

pub const TOKEN_PREFIX: &str = "ag_";
pub const TOKEN_BYTES: usize = 32;
pub const TOKEN_TTL_SECS: i64 = 3600;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AgentRunTokenRow {
    pub token_hash: String,
    pub agent: String,
    pub project_id: Option<i64>,
    pub card_id: Option<i64>,
    pub thread_id: Option<i64>,
    pub machine: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewAgentRunToken<'a> {
    pub agent: &'a str,
    pub project_id: Option<i64>,
    pub card_id: Option<i64>,
    pub thread_id: Option<i64>,
    pub machine: &'a str,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ActiveAgentRun {
    pub agent: String,
    pub project_id: Option<i64>,
    pub card_id: Option<i64>,
    pub thread_id: Option<i64>,
    pub machine: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

pub struct IssuedAgentToken {
    pub token: String,
    pub row: AgentRunTokenRow,
}
