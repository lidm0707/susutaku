//! Domain service bound 1:1 to the AgentConfigRepo port.

use std::sync::Arc;

use kanban_rs::{AgentConfigRow, StoreError};

use crate::domain::AgentConfigDraft;
use crate::port::outbound::AgentConfigRepo;

pub struct AgentConfigService {
    repo: Arc<dyn AgentConfigRepo>,
}

impl AgentConfigService {
    pub fn new(repo: Arc<dyn AgentConfigRepo>) -> Self {
        Self { repo }
    }

    pub async fn list(&self) -> Result<Vec<AgentConfigRow>, StoreError> {
        self.repo.list().await
    }

    pub async fn create(&self, cfg: AgentConfigDraft) -> Result<AgentConfigRow, StoreError> {
        self.repo.create(cfg).await
    }

    pub async fn update(&self, id: i64, cfg: AgentConfigDraft) -> Result<(), StoreError> {
        self.repo.update(id, cfg).await
    }

    pub async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.repo.remove(id).await
    }
}
