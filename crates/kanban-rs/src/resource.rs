//! Pipeline run results stored per card: output_resource node writes here.

use crate::store::{DbTx, Store, StoreError};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ResourceRow {
    pub id: i64,
    pub card_id: i64,
    pub name: String,
    pub content: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct UpsertResource<'a> {
    pub card_id: i64,
    pub name: &'a str,
    pub content: &'a str,
}

impl Store {
    pub async fn upsert_resource(&self, res: UpsertResource<'_>) -> Result<(), StoreError> {
        sqlx::query!(
            r#"INSERT INTO pipeline_resources (card_id, name, content) VALUES ($1, $2, $3)
               ON CONFLICT (card_id, name) DO UPDATE SET content = EXCLUDED.content, created_at = now()"#,
            res.card_id,
            res.name,
            res.content,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn upsert_resource_tx(
        &self,
        tx: &mut DbTx,
        res: UpsertResource<'_>,
    ) -> Result<(), StoreError> {
        sqlx::query!(
            r#"INSERT INTO pipeline_resources (card_id, name, content) VALUES ($1, $2, $3)
               ON CONFLICT (card_id, name) DO UPDATE SET content = EXCLUDED.content, created_at = now()"#,
            res.card_id,
            res.name,
            res.content,
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn list_resources(&self, card_id: i64) -> Result<Vec<ResourceRow>, StoreError> {
        let rows = sqlx::query_as!(
            ResourceRow,
            r#"SELECT id AS "id: i64", card_id AS "card_id: i64", name, content, created_at
               FROM pipeline_resources WHERE card_id = $1 ORDER BY id"#,
            card_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}
