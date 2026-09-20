//! Postgres adapter for the `run_events` table: the per-card agent run
//! stream (generation, tool calls, command output) — full text, persisted
//! so a card view can replay a run after reconnect.

use task_rs::{RunEventRow, StoreError};

use super::Store;

impl Store {
    /// Append one run event; returns its id (the stream cursor).
    pub async fn insert_run_event(
        &self,
        card_id: i64,
        kind: &str,
        text: &str,
    ) -> Result<i64, StoreError> {
        let row = sqlx::query_as!(
            NewId,
            r#"INSERT INTO run_events (card_id, kind, text) VALUES ($1, $2, $3)
               RETURNING id AS "id: i64""#,
            card_id,
            kind,
            text,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.id)
    }

    /// Stream events for a card in order, starting after `after_id`
    /// (0 = from the beginning), capped at `limit` rows.
    pub async fn list_run_events(
        &self,
        card_id: i64,
        after_id: i64,
        limit: i64,
    ) -> Result<Vec<RunEventRow>, StoreError> {
        let rows = sqlx::query_as!(
            RunEventRow,
            r#"SELECT id, card_id, kind, text, created_at
               FROM run_events WHERE card_id = $1 AND id > $2
               ORDER BY id ASC LIMIT $3"#,
            card_id,
            after_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

#[derive(Debug)]
struct NewId {
    id: i64,
}
