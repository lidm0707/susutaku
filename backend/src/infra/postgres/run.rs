//! Postgres adapter for the `run_records` table.

use task_rs::{RunRecordNew, RunRecordRow, StoreError};

use super::Store;

impl Store {
    /// Append a finished run: one `run_records` row plus the card's
    /// `run_status` / `last_agent` / `last_run_id` record fields.
    pub async fn record_run(&self, r: RunRecordNew) -> Result<i64, StoreError> {
        let status = if r.ok {
            task_rs::RUN_STATUS_FINISHED
        } else {
            task_rs::RUN_STATUS_ERROR
        };
        let mut tx = self.begin().await?;
        let row = sqlx::query_as!(
            NewId,
            r#"INSERT INTO run_records (card_id, "trigger", agent, ok, summary, finished_at)
               VALUES ($1, $2, $3, $4, $5, now())
               RETURNING id AS "id: i64""#,
            r.card_id,
            r.trigger.as_str(),
            r.agent.as_str(),
            r.ok,
            r.summary.as_str(),
        )
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query!(
            r#"UPDATE task_cards
               SET run_status = $2, last_agent = $3, last_run_id = $4
               WHERE id = $1"#,
            r.card_id,
            status,
            r.agent,
            row.id,
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row.id)
    }

    /// Mark a run as started: `run_status` running plus the executing agent,
    /// written before inference so watchers see the card is live.
    pub async fn set_run_start(&self, card_id: i64, agent: &str) -> Result<(), StoreError> {
        let res = sqlx::query!(
            r#"UPDATE task_cards SET run_status = $2, last_agent = $3 WHERE id = $1"#,
            card_id,
            task_rs::RUN_STATUS_RUNNING,
            agent,
        )
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchCard);
        }
        Ok(())
    }

    /// Startup recovery: after a restart no run can still be in flight, so
    /// any card left `queued`/`running` by a previous process is reset to
    /// `idle`.
    pub async fn reset_stale_runs(&self) -> Result<u64, StoreError> {
        let res = sqlx::query!(
            r#"UPDATE task_cards SET run_status = $2
               WHERE run_status IN ($1, $3)"#,
            task_rs::RUN_STATUS_QUEUED,
            task_rs::RUN_STATUS_IDLE,
            task_rs::RUN_STATUS_RUNNING,
        )
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    /// Run history for a card, newest first.
    pub async fn card_runs(&self, card_id: i64) -> Result<Vec<RunRecordRow>, StoreError> {
        let rows = sqlx::query_as!(
            RunRecordRow,
            r#"SELECT id, card_id, "trigger", agent, started_at, finished_at, ok, summary
               FROM run_records WHERE card_id = $1 ORDER BY id DESC"#,
            card_id
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
