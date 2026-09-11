//! Kanban outbound ports: repo traits the app layer depends on, implemented
//! by the Postgres adapter.

use async_trait::async_trait;
use kanban_rs::resource::UpsertResource;
use kanban_rs::{
    AgentConfigRow, AgentState, CardRow, CommentRow, PipelineRow, ProjectRow, ResourceRow,
    StoreError, WorkspaceRow,
};
use mockall::automock;

use crate::domain::{
    AgentConfigDraft, CardMove, CardPatch, NewCard, NewPipeline, NewProject, NewWorkspace,
};

#[automock]
#[async_trait]
pub trait WorkspaceRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<WorkspaceRow>, StoreError>;
    async fn create(&self, ws: NewWorkspace) -> Result<WorkspaceRow, StoreError>;
    async fn remove(&self, id: i64) -> Result<(), StoreError>;
}

#[automock]
#[async_trait]
pub trait ProjectRepo: Send + Sync {
    async fn list(&self, workspace_id: i64) -> Result<Vec<ProjectRow>, StoreError>;
    async fn create(&self, p: NewProject) -> Result<ProjectRow, StoreError>;
    async fn remove(&self, id: i64) -> Result<(), StoreError>;
}

#[automock]
#[async_trait]
pub trait CardRepo: Send + Sync {
    async fn list(&self, project_id: Option<i64>) -> Result<Vec<CardRow>, StoreError>;
    async fn get(&self, id: i64) -> Result<Option<CardRow>, StoreError>;
    async fn move_card(&self, mv: CardMove) -> Result<(), StoreError>;
    async fn remove(&self, id: i64) -> Result<(), StoreError>;
    async fn set_agent(&self, id: i64, agent: &AgentState) -> Result<(), StoreError>;
    async fn agent(&self, id: i64) -> Result<Option<AgentState>, StoreError>;
    async fn set_pipeline(&self, card_id: i64, pipeline_id: Option<i64>) -> Result<(), StoreError>;
    async fn set_cron(&self, card_id: i64, cron: Option<String>) -> Result<(), StoreError>;
    /// Opens a transaction handle; write + read-back run inside it.
    async fn tx(&self) -> Result<Box<dyn CardTx>, StoreError>;
}

#[automock]
#[async_trait]
pub trait CardTx: Send {
    async fn add(&mut self, card: NewCard) -> Result<i64, StoreError>;
    async fn get(&mut self, id: i64) -> Result<Option<CardRow>, StoreError>;
    async fn update(&mut self, patch: CardPatch) -> Result<(), StoreError>;
    async fn commit(self: Box<Self>) -> Result<(), StoreError>;
    async fn rollback(self: Box<Self>) -> Result<(), StoreError>;
}

#[automock]
#[async_trait]
pub trait CommentRepo: Send + Sync {
    async fn list(&self, card_id: i64) -> Result<Vec<CommentRow>, StoreError>;
    async fn tx(&self) -> Result<Box<dyn CommentTx>, StoreError>;
}

#[automock]
#[async_trait]
pub trait CommentTx: Send {
    async fn card_exists(&mut self, card_id: i64) -> Result<bool, StoreError>;
    /// Inserts and returns the persisted row (author, body, created_at filled in).
    async fn add(
        &mut self,
        card_id: i64,
        author: &str,
        body: &str,
    ) -> Result<CommentRow, StoreError>;
    async fn commit(self: Box<Self>) -> Result<(), StoreError>;
    async fn rollback(self: Box<Self>) -> Result<(), StoreError>;
}

#[automock]
#[async_trait]
pub trait PipelineRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<PipelineRow>, StoreError>;
    async fn update(&self, id: i64, p: NewPipeline) -> Result<(), StoreError>;
    async fn remove(&self, id: i64) -> Result<(), StoreError>;
    async fn tx(&self) -> Result<Box<dyn PipelineTx>, StoreError>;
}

#[automock]
#[async_trait]
pub trait PipelineTx: Send {
    async fn create(&mut self, p: NewPipeline) -> Result<i64, StoreError>;
    async fn get(&mut self, id: i64) -> Result<Option<PipelineRow>, StoreError>;
    async fn commit(self: Box<Self>) -> Result<(), StoreError>;
    async fn rollback(self: Box<Self>) -> Result<(), StoreError>;
}

#[automock]
#[async_trait]
pub trait ResourceRepo: Send + Sync {
    async fn upsert<'a>(&self, res: UpsertResource<'a>) -> Result<(), StoreError>;
    async fn list(&self, card_id: i64) -> Result<Vec<ResourceRow>, StoreError>;
}

#[automock]
#[async_trait]
pub trait AgentConfigRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<AgentConfigRow>, StoreError>;
    async fn create(&self, cfg: AgentConfigDraft) -> Result<AgentConfigRow, StoreError>;
    async fn update(&self, id: i64, cfg: AgentConfigDraft) -> Result<(), StoreError>;
    async fn remove(&self, id: i64) -> Result<(), StoreError>;
}
