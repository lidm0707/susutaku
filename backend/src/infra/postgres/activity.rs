//! Postgres adapter for the `activity` table.

use task_rs::{ActivityRow, StoreError};

use super::Store;

pub use task_rs::{ACTIVITY_LIST_MAX, ACTIVITY_MESSAGE_MAX_CHARS};

impl Store {
    pub async fn record_activity(&self, kind: &str, message: &str) -> Result<(), StoreError> {
        sqlx::query!(
            r#"INSERT INTO activity (kind, message) VALUES ($1, $2)"#,
            kind,
            clamp_message(message),
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
}

fn clamp_message(message: &str) -> String {
    message.chars().take(ACTIVITY_MESSAGE_MAX_CHARS).collect()
}
