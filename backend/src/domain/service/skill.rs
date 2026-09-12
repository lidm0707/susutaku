//! Domain service bound 1:1 to the SkillRepo port.

use std::sync::Arc;

use kanban_rs::{SkillRow, StoreError};

use crate::port::outbound::SkillRepo;

pub struct SkillService {
    repo: Arc<dyn SkillRepo>,
}

impl SkillService {
    pub fn new(repo: Arc<dyn SkillRepo>) -> Self {
        Self { repo }
    }

    pub async fn list(&self) -> Result<Vec<SkillRow>, StoreError> {
        self.repo.list().await
    }

    pub async fn create(&self, name: &str, body: &str) -> Result<SkillRow, StoreError> {
        self.repo.create(name, body).await
    }

    pub async fn update(&self, id: i64, body: &str) -> Result<(), StoreError> {
        self.repo.update(id, body).await
    }

    pub async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.repo.remove(id).await
    }

    pub async fn list_for_agent(&self, agent_id: i64) -> Result<Vec<SkillRow>, StoreError> {
        self.repo.list_for_agent(agent_id).await
    }

    pub async fn attach(&self, agent_id: i64, skill_id: i64) -> Result<(), StoreError> {
        self.repo.attach(agent_id, skill_id).await
    }

    pub async fn detach(&self, agent_id: i64, skill_id: i64) -> Result<(), StoreError> {
        self.repo.detach(agent_id, skill_id).await
    }
}
