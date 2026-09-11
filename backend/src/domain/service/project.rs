//! Domain service bound 1:1 to the ProjectRepo port.

use std::sync::Arc;

use kanban_rs::{ProjectRow, StoreError};

use crate::domain::NewProject;
use crate::port::outbound::ProjectRepo;

pub struct ProjectService {
    repo: Arc<dyn ProjectRepo>,
}

impl ProjectService {
    pub fn new(repo: Arc<dyn ProjectRepo>) -> Self {
        Self { repo }
    }

    pub async fn list(&self, workspace_id: i64) -> Result<Vec<ProjectRow>, StoreError> {
        self.repo.list(workspace_id).await
    }

    pub async fn create(&self, p: NewProject) -> Result<ProjectRow, StoreError> {
        self.repo.create(p).await
    }

    pub async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.repo.remove(id).await
    }
}
