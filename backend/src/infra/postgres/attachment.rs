//! Postgres adapter for the `card_attachments` table: per-card links to
//! uploaded files on disk.

use std::collections::HashMap;

use task_rs::{AttachmentLinkRow, CardRow, StoreError};

use super::Store;

/// Link an uploaded file to a card. Idempotent per (card, path).
pub async fn link(store: &Store, card_id: i64, path: &str, name: &str) -> Result<(), StoreError> {
    store.link_attachment(card_id, path, name).await
}

/// All attachment links.
pub async fn links(store: &Store) -> Result<Vec<AttachmentLinkRow>, StoreError> {
    store.list_attachment_links().await
}

/// Cards linked to attachments via `card_attachments`, keyed by file path.
/// `cards` supplies the card rows; links pointing at missing cards are
/// dropped.
pub async fn linked_cards(
    store: &Store,
    cards: &[CardRow],
) -> Result<HashMap<String, Vec<CardRow>>, StoreError> {
    let by_id: HashMap<i64, &CardRow> = cards.iter().map(|c| (c.id, c)).collect();
    let mut map: HashMap<String, Vec<CardRow>> = HashMap::new();
    for link in store.list_attachment_links().await? {
        if let Some(card) = by_id.get(&link.card_id) {
            map.entry(link.path).or_default().push((*card).clone());
        }
    }
    Ok(map)
}

/// Drop every link to a path (file deleted).
pub async fn delete_links(store: &Store, path: &str) -> Result<(), StoreError> {
    store.delete_attachment_links(path).await
}

impl Store {
    pub async fn link_attachment(
        &self,
        card_id: i64,
        path: &str,
        name: &str,
    ) -> Result<(), StoreError> {
        sqlx::query!(
            r#"INSERT INTO card_attachments (card_id, path, name) VALUES ($1, $2, $3)
               ON CONFLICT (card_id, path) DO NOTHING"#,
            card_id,
            path,
            name,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_attachment_links(&self) -> Result<Vec<AttachmentLinkRow>, StoreError> {
        let rows = sqlx::query_as!(
            AttachmentLinkRow,
            r#"SELECT id AS "id: i64", card_id AS "card_id: i64", path, name, created_at
               FROM card_attachments ORDER BY id"#
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn list_card_attachments(
        &self,
        card_id: i64,
    ) -> Result<Vec<AttachmentLinkRow>, StoreError> {
        let rows = sqlx::query_as!(
            AttachmentLinkRow,
            r#"SELECT id AS "id: i64", card_id AS "card_id: i64", path, name, created_at
               FROM card_attachments WHERE card_id = $1 ORDER BY id"#,
            card_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn unlink_attachment(&self, card_id: i64, path: &str) -> Result<(), StoreError> {
        sqlx::query!(
            r#"DELETE FROM card_attachments WHERE card_id = $1 AND path = $2"#,
            card_id,
            path,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_attachment_links(&self, path: &str) -> Result<(), StoreError> {
        sqlx::query!(r#"DELETE FROM card_attachments WHERE path = $1"#, path)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
