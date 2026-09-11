//! Domain service bound 1:1 to the WorkspaceRepo port.

use std::sync::Arc;

use kanban_rs::{StoreError, WorkspaceRow};

use crate::domain::NewWorkspace;
use crate::port::outbound::WorkspaceRepo;

pub struct WorkspaceService {
    repo: Arc<dyn WorkspaceRepo>,
}

impl WorkspaceService {
    pub fn new(repo: Arc<dyn WorkspaceRepo>) -> Self {
        Self { repo }
    }

    pub async fn list(&self) -> Result<Vec<WorkspaceRow>, StoreError> {
        self.repo.list().await
    }

    pub async fn create(&self, ws: NewWorkspace) -> Result<WorkspaceRow, StoreError> {
        self.repo.create(ws).await
    }

    pub async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.repo.remove(id).await
    }
}
