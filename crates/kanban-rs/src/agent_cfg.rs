//! Per-agent configuration (model, persona, prompt, output) in Postgres.

use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

const UNIQUE_VIOLATION: &str = "23505";

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentConfigRow {
    pub id: i64,
    pub name: String,
    pub model: String,
    pub persona: String,
    pub prompt: String,
    pub output: String,
}

pub struct AgentConfigUpdate<'a> {
    pub id: i64,
    pub name: &'a str,
    pub model: &'a str,
    pub persona: &'a str,
    pub prompt: &'a str,
    pub output: &'a str,
}

impl Store {
    pub async fn list_agents(&self) -> Result<Vec<AgentConfigRow>, StoreError> {
        let rows = sqlx::query_as!(
            AgentConfigRow,
            r#"SELECT id, name, model, persona, prompt, output FROM agent_settings ORDER BY id"#
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn create_agent(&self, cfg: &AgentConfigRow) -> Result<i64, StoreError> {
        let row = sqlx::query!(
            r#"INSERT INTO agent_settings (name, model, persona, prompt, output)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING id AS "id: i64""#,
            cfg.name,
            cfg.model,
            cfg.persona,
            cfg.prompt,
            cfg.output,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e.as_database_error() {
            Some(d) if d.code().as_deref() == Some(UNIQUE_VIOLATION) => StoreError::AgentTaken,
            _ => StoreError::Db(e),
        })?;
        Ok(row.id)
    }

    pub async fn update_agent(&self, upd: AgentConfigUpdate<'_>) -> Result<(), StoreError> {
        let res = sqlx::query!(
            r#"UPDATE agent_settings
               SET name = $2, model = $3, persona = $4, prompt = $5, output = $6
               WHERE id = $1"#,
            upd.id,
            upd.name,
            upd.model,
            upd.persona,
            upd.prompt,
            upd.output,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| match e.as_database_error() {
            Some(d) if d.code().as_deref() == Some(UNIQUE_VIOLATION) => StoreError::AgentTaken,
            _ => StoreError::Db(e),
        })?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchAgent);
        }
        Ok(())
    }

    pub async fn remove_agent(&self, id: i64) -> Result<(), StoreError> {
        let res = sqlx::query!(r#"DELETE FROM agent_settings WHERE id = $1"#, id)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchAgent);
        }
        Ok(())
    }
}
