//! Domain service bound 1:1 to the PipelineRepo port.

use std::sync::Arc;

use kanban_rs::{PipelineRow, StoreError};

use crate::domain::NewPipeline;
use crate::port::outbound::PipelineRepo;

pub struct PipelineService {
    repo: Arc<dyn PipelineRepo>,
}

impl PipelineService {
    pub fn new(repo: Arc<dyn PipelineRepo>) -> Self {
        Self { repo }
    }

    pub async fn list(&self) -> Result<Vec<PipelineRow>, StoreError> {
        self.repo.list().await
    }

    /// Create + read-back inside one transaction.
    pub async fn create(&self, pipeline: NewPipeline) -> Result<PipelineRow, StoreError> {
        let mut tx = self.repo.tx().await?;
        let id = tx.create(pipeline).await?;
        let row = tx.get(id).await?.ok_or(StoreError::NoSuchPipeline)?;
        tx.commit().await?;
        Ok(row)
    }

    pub async fn update(&self, id: i64, pipeline: NewPipeline) -> Result<(), StoreError> {
        self.repo.update(id, pipeline).await
    }

    pub async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.repo.remove(id).await
    }
}
