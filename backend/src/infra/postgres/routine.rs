//! Postgres adapter for the `routines` + `routine_runs` tables.

use task_rs::{RoutineDraft, RoutineRow, RoutineRunRow, StoreError};

use super::Store;

impl Store {
    pub async fn list_routines(&self) -> Result<Vec<RoutineRow>, StoreError> {
        let rows = sqlx::query!(
            r#"SELECT id, name, cron, agent, instruction, enabled,
                      to_char(created_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS "created_at!"
               FROM routines ORDER BY id"#
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| RoutineRow {
                id: r.id,
                name: r.name,
                cron: r.cron,
                agent: r.agent,
                instruction: r.instruction,
                enabled: r.enabled,
                created_at: r.created_at,
            })
            .collect())
    }

    pub async fn get_routine(&self, id: i64) -> Result<Option<RoutineRow>, StoreError> {
        let rows = self.list_routines().await?;
        Ok(rows.into_iter().find(|r| r.id == id))
    }

    pub async fn add_routine(&self, d: &RoutineDraft) -> Result<i64, StoreError> {
        let rec = sqlx::query!(
            r#"INSERT INTO routines (name, cron, agent, instruction, enabled)
               VALUES ($1, $2, $3, $4, $5) RETURNING id AS "id!""#,
            d.name,
            d.cron,
            d.agent,
            d.instruction,
            d.enabled,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(rec.id)
    }

    pub async fn update_routine(&self, id: i64, d: &RoutineDraft) -> Result<(), StoreError> {
        let res = sqlx::query!(
            r#"UPDATE routines
               SET name = $2, cron = $3, agent = $4, instruction = $5, enabled = $6
               WHERE id = $1"#,
            id,
            d.name,
            d.cron,
            d.agent,
            d.instruction,
            d.enabled,
        )
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchRoutine);
        }
        Ok(())
    }

    pub async fn remove_routine(&self, id: i64) -> Result<(), StoreError> {
        let res = sqlx::query!(r#"DELETE FROM routines WHERE id = $1"#, id)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchRoutine);
        }
        Ok(())
    }

    pub async fn record_routine_run(
        &self,
        routine_id: i64,
        trigger: &str,
        ok: bool,
        summary: &str,
    ) -> Result<i64, StoreError> {
        const SUMMARY_CHARS: usize = 400;
        let summary: String = summary.chars().take(SUMMARY_CHARS).collect();
        let rec = sqlx::query!(
            r#"INSERT INTO routine_runs (routine_id, "trigger", finished_at, ok, summary)
               VALUES ($1, $2, now(), $3, $4) RETURNING id AS "id!""#,
            routine_id,
            trigger,
            ok,
            summary,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(rec.id)
    }

    pub async fn routine_runs(&self, routine_id: i64) -> Result<Vec<RoutineRunRow>, StoreError> {
        let rows = sqlx::query!(
            r#"SELECT id, routine_id, "trigger",
                      to_char(started_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS "started_at!",
                      to_char(finished_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS finished_at,
                      ok, summary
               FROM routine_runs WHERE routine_id = $1 ORDER BY id DESC"#,
            routine_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| RoutineRunRow {
                id: r.id,
                routine_id: r.routine_id,
                trigger: r.trigger,
                started_at: r.started_at,
                finished_at: r.finished_at,
                ok: r.ok,
                summary: r.summary,
            })
            .collect())
    }
}
