//! Postgres adapter for the `task_comments` table.

use async_trait::async_trait;
use task_rs::{CommentRow, StoreError};

use super::{DbTx, PgTask, Store, commit_tx, rollback_tx};
use crate::port::outbound::{CommentRepo, CommentTx};

impl Store {
    pub async fn list_comments(&self, card_id: i64) -> Result<Vec<CommentRow>, StoreError> {
        let rows = sqlx::query_as!(
            CommentRow,
            r#"SELECT id, card_id, author, body, created_at
               FROM task_comments WHERE card_id = $1 ORDER BY id"#,
            card_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn add_comment(
        &self,
        card_id: i64,
        author: &str,
        body: &str,
    ) -> Result<i64, StoreError> {
        let mut tx = self.begin().await?;
        if !self.card_exists_tx(&mut tx, card_id).await? {
            return Err(StoreError::NoSuchCard);
        }
        let id = self
            .add_comment_tx(&mut tx, card_id, author, body)
            .await?
            .id;
        tx.commit().await?;
        Ok(id)
    }

    /// Inserts a comment and returns the full row (id, author, body, created_at).
    pub async fn add_comment_tx(
        &self,
        tx: &mut DbTx,
        card_id: i64,
        author: &str,
        body: &str,
    ) -> Result<CommentRow, StoreError> {
        let row = sqlx::query_as!(
            CommentRow,
            r#"INSERT INTO task_comments (card_id, author, body)
               VALUES ($1, $2, $3)
               RETURNING id, card_id, author, body, created_at"#,
            card_id,
            author,
            body,
        )
        .fetch_one(&mut **tx)
        .await?;
        Ok(row)
    }
}

#[async_trait]
impl CommentRepo for PgTask {
    async fn list(&self, card_id: i64) -> Result<Vec<CommentRow>, StoreError> {
        self.store().list_comments(card_id).await
    }

    async fn tx(&self) -> Result<Box<dyn CommentTx>, StoreError> {
        Ok(Box::new(PgCommentTx {
            store: std::sync::Arc::clone(&self.store),
            tx: self.store().begin().await?,
        }))
    }
}

struct PgCommentTx {
    store: std::sync::Arc<Store>,
    tx: DbTx,
}

#[async_trait]
impl CommentTx for PgCommentTx {
    async fn card_exists(&mut self, card_id: i64) -> Result<bool, StoreError> {
        self.store.card_exists_tx(&mut self.tx, card_id).await
    }

    async fn add(
        &mut self,
        card_id: i64,
        author: &str,
        body: &str,
    ) -> Result<CommentRow, StoreError> {
        self.store
            .add_comment_tx(&mut self.tx, card_id, author, body)
            .await
    }

    async fn commit(self: Box<Self>) -> Result<(), StoreError> {
        commit_tx(self.tx).await
    }

    async fn rollback(self: Box<Self>) -> Result<(), StoreError> {
        rollback_tx(self.tx).await
    }
}
