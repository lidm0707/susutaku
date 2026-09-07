//! Pipeline definitions persisted in Postgres; spec JSON validated via piplines.

use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

const UNIQUE_VIOLATION: &str = "23505";

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PipelineRow {
    pub id: i64,
    pub name: String,
    pub spec: String,
}

fn validate_spec(spec: &str) -> Result<(), StoreError> {
    let parsed: piplines::graph::PipelineSpec = serde_json::from_str(spec)
        .map_err(|e| StoreError::BadSpec(e.to_string()))?;
    parsed.validate().map_err(|e| StoreError::BadSpec(e.to_string()))
}

impl Store {
    pub async fn list_pipelines(&self) -> Result<Vec<PipelineRow>, StoreError> {
        let rows = sqlx::query_as!(
            PipelineRow,
            r#"SELECT id, name, spec FROM pipelines ORDER BY id"#
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn create_pipeline(&self, name: &str, spec: &str) -> Result<i64, StoreError> {
        validate_spec(spec)?;
        let row = sqlx::query!(
            r#"INSERT INTO pipelines (name, spec) VALUES ($1, $2) RETURNING id AS "id: i64""#,
            name,
            spec,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e.as_database_error() {
            Some(d) if d.code().as_deref() == Some(UNIQUE_VIOLATION) => StoreError::PipelineTaken,
            _ => StoreError::Db(e),
        })?;
        Ok(row.id)
    }

    pub async fn update_pipeline(&self, id: i64, name: &str, spec: &str) -> Result<(), StoreError> {
        validate_spec(spec)?;
        let res = sqlx::query!(
            r#"UPDATE pipelines SET name = $2, spec = $3 WHERE id = $1"#,
            id,
            name,
            spec,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| match e.as_database_error() {
            Some(d) if d.code().as_deref() == Some(UNIQUE_VIOLATION) => StoreError::PipelineTaken,
            _ => StoreError::Db(e),
        })?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchPipeline);
        }
        Ok(())
    }

    pub async fn remove_pipeline(&self, id: i64) -> Result<(), StoreError> {
        let res = sqlx::query!(r#"DELETE FROM pipelines WHERE id = $1"#, id)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchPipeline);
        }
        Ok(())
    }
}
