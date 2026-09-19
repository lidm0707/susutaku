//! Postgres adapter for the `agent_settings` table.

use async_trait::async_trait;
use task_rs::{AgentConfigRow, AgentConfigUpdate, StoreError};

use super::{PgTask, Store};
use crate::domain::AgentConfigDraft;
use crate::port::outbound::AgentConfigRepo;

const UNIQUE_VIOLATION: &str = "23505";

impl Store {
    pub async fn list_agents(&self) -> Result<Vec<AgentConfigRow>, StoreError> {
        let rows = sqlx::query_as!(
            AgentConfigRow,
            r#"SELECT id, name, model, persona, prompt, output, allowed_tools, receive_images, thinking, ctx_limit, ctx_policy
               FROM agent_settings ORDER BY id"#
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn agent_by_name(&self, name: &str) -> Result<Option<AgentConfigRow>, StoreError> {
        let row = sqlx::query_as!(
            AgentConfigRow,
            r#"SELECT id, name, model, persona, prompt, output, allowed_tools, receive_images, thinking, ctx_limit, ctx_policy
               FROM agent_settings WHERE name = $1"#,
            name
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn create_agent(&self, cfg: &AgentConfigRow) -> Result<i64, StoreError> {
        let row = sqlx::query!(
            r#"INSERT INTO agent_settings (name, model, persona, prompt, output, allowed_tools, receive_images, thinking, ctx_limit, ctx_policy)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
               RETURNING id AS "id: i64""#,
            cfg.name,
            cfg.model,
            cfg.persona,
            cfg.prompt,
            cfg.output,
            &cfg.allowed_tools,
            cfg.receive_images,
            cfg.thinking,
            cfg.ctx_limit,
            cfg.ctx_policy,
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
               SET name = $2, model = $3, persona = $4, prompt = $5, output = $6,
                   allowed_tools = $7, receive_images = $8, thinking = $9,
                   ctx_limit = $10, ctx_policy = $11
               WHERE id = $1"#,
            upd.id,
            upd.name,
            upd.model,
            upd.persona,
            upd.prompt,
            upd.output,
            upd.allowed_tools,
            upd.receive_images,
            upd.thinking,
            upd.ctx_limit,
            upd.ctx_policy,
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

#[async_trait]
impl AgentConfigRepo for PgTask {
    async fn list(&self) -> Result<Vec<AgentConfigRow>, StoreError> {
        self.store().list_agents().await
    }

    async fn by_name(&self, name: &str) -> Result<Option<AgentConfigRow>, StoreError> {
        self.store().agent_by_name(name).await
    }

    async fn create(&self, cfg: AgentConfigDraft) -> Result<AgentConfigRow, StoreError> {
        let row = AgentConfigRow {
            id: 0,
            name: cfg.name,
            model: cfg.model,
            persona: cfg.persona,
            prompt: cfg.prompt,
            output: cfg.output,
            allowed_tools: cfg.allowed_tools,
            receive_images: cfg.receive_images,
            thinking: cfg.thinking,
            ctx_limit: cfg.ctx_limit,
            ctx_policy: cfg.ctx_policy,
        };
        let id = self.store().create_agent(&row).await?;
        Ok(AgentConfigRow { id, ..row })
    }

    async fn update(&self, id: i64, cfg: AgentConfigDraft) -> Result<(), StoreError> {
        let upd = AgentConfigUpdate {
            id,
            name: Box::leak(cfg.name.into_boxed_str()),
            model: Box::leak(cfg.model.into_boxed_str()),
            persona: Box::leak(cfg.persona.into_boxed_str()),
            prompt: Box::leak(cfg.prompt.into_boxed_str()),
            output: Box::leak(cfg.output.into_boxed_str()),
            allowed_tools: Box::leak(cfg.allowed_tools.into_boxed_slice()),
            receive_images: cfg.receive_images,
            thinking: Box::leak(cfg.thinking.into_boxed_str()),
            ctx_limit: cfg.ctx_limit,
            ctx_policy: Box::leak(cfg.ctx_policy.into_boxed_str()),
        };
        self.store().update_agent(upd).await
    }

    async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.store().remove_agent(id).await
    }
}
