//! Per-agent configuration (model, persona, prompt, output, allowed tools) in
//! Postgres.

use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

const UNIQUE_VIOLATION: &str = "23505";

/// Custom reasoning depth for an agent, resolved at chat time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThinkLevel {
    #[default]
    Off,
    Low,
    Medium,
    High,
}

impl ThinkLevel {
    pub const ALL: [ThinkLevel; 4] = [
        ThinkLevel::Off,
        ThinkLevel::Low,
        ThinkLevel::Medium,
        ThinkLevel::High,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ThinkLevel::Off => "off",
            ThinkLevel::Low => "low",
            ThinkLevel::Medium => "medium",
            ThinkLevel::High => "high",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Some(ThinkLevel::Off),
            "low" => Some(ThinkLevel::Low),
            "medium" => Some(ThinkLevel::Medium),
            "high" => Some(ThinkLevel::High),
            _ => None,
        }
    }

    pub fn on(self) -> bool {
        self != ThinkLevel::Off
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentConfigRow {
    pub id: i64,
    pub name: String,
    pub model: String,
    pub persona: String,
    pub prompt: String,
    pub output: String,
    pub allowed_tools: Vec<String>,
    /// False = the agent must never receive images (screenshots, attachments).
    pub receive_images: bool,
    /// Reasoning depth (ThinkLevel::as_str); "off" = no thinking block.
    pub thinking: String,
}

impl AgentConfigRow {
    pub fn think_level(&self) -> ThinkLevel {
        ThinkLevel::parse(&self.thinking).unwrap_or_default()
    }
}

pub struct AgentConfigUpdate<'a> {
    pub id: i64,
    pub name: &'a str,
    pub model: &'a str,
    pub persona: &'a str,
    pub prompt: &'a str,
    pub output: &'a str,
    pub allowed_tools: &'a [String],
    pub receive_images: bool,
    pub thinking: &'a str,
}

impl Store {
    pub async fn list_agents(&self) -> Result<Vec<AgentConfigRow>, StoreError> {
        let rows = sqlx::query_as!(
            AgentConfigRow,
            r#"SELECT id, name, model, persona, prompt, output, allowed_tools, receive_images, thinking
               FROM agent_settings ORDER BY id"#
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn agent_by_name(&self, name: &str) -> Result<Option<AgentConfigRow>, StoreError> {
        let row = sqlx::query_as!(
            AgentConfigRow,
            r#"SELECT id, name, model, persona, prompt, output, allowed_tools, receive_images, thinking
               FROM agent_settings WHERE name = $1"#,
            name
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn create_agent(&self, cfg: &AgentConfigRow) -> Result<i64, StoreError> {
        let row = sqlx::query!(
            r#"INSERT INTO agent_settings (name, model, persona, prompt, output, allowed_tools, receive_images, thinking)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING id AS "id: i64""#,
            cfg.name,
            cfg.model,
            cfg.persona,
            cfg.prompt,
            cfg.output,
            &cfg.allowed_tools,
            cfg.receive_images,
            cfg.thinking,
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
                   allowed_tools = $7, receive_images = $8, thinking = $9
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
