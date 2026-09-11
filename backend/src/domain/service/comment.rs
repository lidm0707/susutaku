//! Domain service bound 1:1 to the CommentRepo port.

use std::sync::Arc;

use kanban_rs::{CommentRow, StoreError};

use crate::port::outbound::CommentRepo;

pub struct CommentService {
    repo: Arc<dyn CommentRepo>,
}

impl CommentService {
    pub fn new(repo: Arc<dyn CommentRepo>) -> Self {
        Self { repo }
    }

    pub async fn list(&self, card_id: i64) -> Result<Vec<CommentRow>, StoreError> {
        self.repo.list(card_id).await
    }

    /// Exists-check + insert inside one transaction.
    pub async fn add(
        &self,
        card_id: i64,
        author: String,
        body: String,
    ) -> Result<CommentRow, StoreError> {
        let mut tx = self.repo.tx().await?;
        if !tx.card_exists(card_id).await? {
            tx.rollback().await?;
            return Err(StoreError::NoSuchCard);
        }
        let row = tx.add(card_id, &author, &body).await?;
        tx.commit().await?;
        Ok(row)
    }
}
