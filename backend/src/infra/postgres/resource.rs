//! Postgres adapter for the `card_resources` table.

use async_trait::async_trait;
use task_rs::{ResourceRow, StoreError, UpsertResource};

use super::{PgTask, Store};
use crate::port::outbound::ResourceRepo;

impl Store {
    pub async fn upsert_resource(&self, res: UpsertResource<'_>) -> Result<(), StoreError> {
        sqlx::query!(
            r#"INSERT INTO card_resources (card_id, name, content) VALUES ($1, $2, $3)
               ON CONFLICT (card_id, name) DO UPDATE SET content = EXCLUDED.content, created_at = now()"#,
            res.card_id,
            res.name,
            res.content,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_resources(&self, card_id: i64) -> Result<Vec<ResourceRow>, StoreError> {
        let rows = sqlx::query_as!(
            ResourceRow,
            r#"SELECT id AS "id: i64", card_id AS "card_id: i64", name, content, created_at
               FROM card_resources WHERE card_id = $1 ORDER BY id"#,
            card_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

#[async_trait]
impl ResourceRepo for PgTask {
    async fn upsert<'a>(&self, res: UpsertResource<'a>) -> Result<(), StoreError> {
        self.store().upsert_resource(res).await
    }

    async fn list(&self, card_id: i64) -> Result<Vec<ResourceRow>, StoreError> {
        self.store().list_resources(card_id).await
    }
}
