//! Domain service bound 1:1 to the ResourceRepo port.

use std::sync::Arc;

use kanban_rs::resource::UpsertResource;
use kanban_rs::{ResourceRow, StoreError};

use crate::port::outbound::ResourceRepo;

pub struct ResourceService {
    repo: Arc<dyn ResourceRepo>,
}

impl ResourceService {
    pub fn new(repo: Arc<dyn ResourceRepo>) -> Self {
        Self { repo }
    }

    pub async fn upsert(&self, res: UpsertResource<'_>) -> Result<(), StoreError> {
        self.repo.upsert(res).await
    }

    pub async fn list(&self, card_id: i64) -> Result<Vec<ResourceRow>, StoreError> {
        self.repo.list(card_id).await
    }
}
