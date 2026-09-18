//! Chat transcript model: threads + messages rows and role consts.
//! Pure model — the SQL lives in the backend's postgres adapter.

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct ChatThreadRow {
    pub id: i64,
    pub agent: String,
    pub title: String,
    pub project_id: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct ChatMessageRow {
    pub id: i64,
    pub thread_id: i64,
    pub role: String,
    pub text: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub const ROLE_USER: &str = "user";
pub const ROLE_ASSISTANT: &str = "assistant";
