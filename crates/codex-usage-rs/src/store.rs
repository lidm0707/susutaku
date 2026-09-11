//! Postgres persistence for usage snapshots (runtime queries — no
//! compile-time DB dependency).

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::snapshot::RateLimits;

pub const TABLE_NAME: &str = "codex_usage";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct Row {
    pub captured_at: DateTime<Utc>,
    pub plan_type: Option<String>,
    pub primary_used_percent: Option<f64>,
    pub primary_resets_at: Option<DateTime<Utc>>,
    pub secondary_used_percent: Option<f64>,
    pub secondary_resets_at: Option<DateTime<Utc>>,
}

pub struct Store {
    pool: PgPool,
}

fn ts(secs: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(secs, 0)
}

impl Store {
    pub async fn connect(url: &str) -> Result<Self, StoreError> {
        Ok(Self {
            pool: PgPool::connect(url).await?,
        })
    }

    pub async fn ensure_table(&self) -> Result<(), StoreError> {
        sqlx::query(&format!(
            "CREATE TABLE IF NOT EXISTS {TABLE_NAME} (
                id BIGSERIAL PRIMARY KEY,
                captured_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                plan_type TEXT,
                primary_used_percent DOUBLE PRECISION,
                primary_resets_at TIMESTAMPTZ,
                secondary_used_percent DOUBLE PRECISION,
                secondary_resets_at TIMESTAMPTZ,
                raw JSONB
            )"
        ))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert(&self, limits: &RateLimits) -> Result<i64, StoreError> {
        let raw = serde_json::to_value(limits).unwrap_or_default();
        let row = sqlx::query_scalar::<_, i64>(&format!(
            "INSERT INTO {TABLE_NAME}
             (plan_type, primary_used_percent, primary_resets_at,
              secondary_used_percent, secondary_resets_at, raw)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id"
        ))
        .bind(limits.plan_type.as_deref())
        .bind(limits.primary.as_ref().map(|w| w.used_percent))
        .bind(limits.primary.as_ref().and_then(|w| ts(w.resets_at)))
        .bind(limits.secondary.as_ref().map(|w| w.used_percent))
        .bind(limits.secondary.as_ref().and_then(|w| ts(w.resets_at)))
        .bind(raw)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn latest(&self) -> Result<Option<Row>, StoreError> {
        let row = sqlx::query_as::<_, Row>(&format!(
            "SELECT captured_at, plan_type,
                    primary_used_percent, primary_resets_at,
                    secondary_used_percent, secondary_resets_at
             FROM {TABLE_NAME} ORDER BY id DESC LIMIT 1"
        ))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn history(&self, limit: i64) -> Result<Vec<Row>, StoreError> {
        let rows = sqlx::query_as::<_, Row>(&format!(
            "SELECT captured_at, plan_type,
                    primary_used_percent, primary_resets_at,
                    secondary_used_percent, secondary_resets_at
             FROM {TABLE_NAME} ORDER BY id DESC LIMIT {limit}"
        ))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}
