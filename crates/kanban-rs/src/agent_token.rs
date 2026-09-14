//! Agent run tickets: temporary tokens carrying run routing context.
//! Plaintext token is returned once; only the SHA-256 hash is stored.

use crate::store::{Store, StoreError};
use argon2::password_hash::rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const TOKEN_PREFIX: &str = "ag_";
pub const TOKEN_BYTES: usize = 32;
pub const TOKEN_TTL_SECS: i64 = 3600;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AgentRunTokenRow {
    pub token_hash: String,
    pub agent: String,
    pub project_id: Option<i64>,
    pub card_id: Option<i64>,
    pub thread_id: Option<i64>,
    pub machine: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewAgentRunToken<'a> {
    pub agent: &'a str,
    pub project_id: Option<i64>,
    pub card_id: Option<i64>,
    pub thread_id: Option<i64>,
    pub machine: &'a str,
}

fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn new_token() -> Result<String, StoreError> {
    let mut bytes = [0u8; TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    Ok(format!(
        "{TOKEN_PREFIX}{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ActiveAgentRun {
    pub agent: String,
    pub project_id: Option<i64>,
    pub card_id: Option<i64>,
    pub thread_id: Option<i64>,
    pub machine: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

pub struct IssuedAgentToken {
    pub token: String,
    pub row: AgentRunTokenRow,
}

impl Store {
    /// Issue a one-shot run ticket; the plaintext token is visible only here.
    pub async fn issue_agent_token(
        &self,
        req: &NewAgentRunToken<'_>,
    ) -> Result<IssuedAgentToken, StoreError> {
        let token = new_token()?;
        let row = sqlx::query_as!(
            AgentRunTokenRow,
            r#"INSERT INTO agent_run_tokens (token_hash, agent, project_id, card_id, thread_id, machine, expires_at)
               VALUES ($1, $2, $3, $4, $5, $6, now() + make_interval(secs => $7))
               RETURNING token_hash, agent, project_id, card_id, thread_id, machine, created_at, expires_at"#,
            hash_token(&token),
            req.agent,
            req.project_id,
            req.card_id,
            req.thread_id,
            req.machine,
            TOKEN_TTL_SECS as f64,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(IssuedAgentToken { token, row })
    }

    /// Resolve a plaintext ticket to its routing context; expired tickets are
    /// invisible (and pruned opportunistically).
    pub async fn resolve_agent_token(
        &self,
        token: &str,
    ) -> Result<Option<AgentRunTokenRow>, StoreError> {
        let row = sqlx::query_as!(
            AgentRunTokenRow,
            r#"SELECT token_hash, agent, project_id, card_id, thread_id, machine, created_at, expires_at
               FROM agent_run_tokens
               WHERE token_hash = $1 AND expires_at > now()"#,
            hash_token(token),
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Consume a one-shot ticket.
    pub async fn consume_agent_token(&self, token_hash: &str) -> Result<(), StoreError> {
        sqlx::query!(
            r#"DELETE FROM agent_run_tokens WHERE token_hash = $1"#,
            token_hash
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn prune_expired_agent_tokens(&self) -> Result<u64, StoreError> {
        let res = sqlx::query!(r#"DELETE FROM agent_run_tokens WHERE expires_at <= now()"#)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    /// Live runs: which agents hold valid tickets, and where they work.
    pub async fn list_active_agent_tokens(&self) -> Result<Vec<ActiveAgentRun>, StoreError> {
        let rows = sqlx::query_as!(
            ActiveAgentRun,
            r#"SELECT agent, project_id, card_id, thread_id, machine, expires_at
               FROM agent_run_tokens WHERE expires_at > now() ORDER BY created_at"#
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}
