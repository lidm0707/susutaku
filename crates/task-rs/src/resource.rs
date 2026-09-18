//! Per-card resources: output nodes write run results here.
//! Pure model — the SQL lives in the backend's postgres adapter.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ResourceRow {
    pub id: i64,
    pub card_id: i64,
    pub name: String,
    pub content: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct UpsertResource<'a> {
    pub card_id: i64,
    pub name: &'a str,
    pub content: &'a str,
}
