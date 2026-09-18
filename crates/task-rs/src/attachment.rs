//! Per-card attachment links: uploaded files on disk referenced by a card.
//! Pure model — the SQL lives in the backend's postgres adapter.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AttachmentLinkRow {
    pub id: i64,
    pub card_id: i64,
    pub path: String,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
