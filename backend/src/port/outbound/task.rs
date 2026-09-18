//! Task outbound ports: repo traits the app layer depends on, implemented
//! by the Postgres adapter.

use async_trait::async_trait;
use mockall::automock;
use task_rs::resource::UpsertResource;
use task_rs::{
    AgentConfigRow, AgentState, CardRow, CommentRow, ProjectRow, ResourceRow, RunRecordNew,
    RunRecordRow, SkillRow, StoreError, WorkspaceRow,
};

use crate::domain::{AgentConfigDraft, CardMove, CardPatch, NewCard, NewProject, NewWorkspace};

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
    /// Cards whose run is queued or running, across all projects.
    async fn running_cards(&self) -> Result<Vec<CardRow>, StoreError>;
    async fn get(&self, id: i64) -> Result<Option<CardRow>, StoreError>;
    async fn move_card(&self, mv: CardMove) -> Result<(), StoreError>;
    async fn remove(&self, id: i64) -> Result<(), StoreError>;
    async fn set_agent(&self, id: i64, agent: &AgentState) -> Result<(), StoreError>;
    /// Writes only the run ledger; the pinned agent stays untouched.
    async fn set_agent_state(&self, id: i64, state_json: &str) -> Result<(), StoreError>;
    async fn agent(&self, id: i64) -> Result<Option<AgentState>, StoreError>;
    /// Appends a finished run and updates the card's run record fields.
    async fn record_run(&self, r: RunRecordNew) -> Result<i64, StoreError>;
    /// Marks a run as started (running + last_agent) before inference.
    async fn set_run_start(&self, card_id: i64, agent: &str) -> Result<(), StoreError>;
    async fn card_runs(&self, card_id: i64) -> Result<Vec<RunRecordRow>, StoreError>;
    async fn set_cron(&self, card_id: i64, cron: Option<String>) -> Result<(), StoreError>;
    /// Sets (or clears with None) the card's sandbox image.
    async fn set_card_image<'a>(&self, id: i64, image: Option<&'a str>) -> Result<(), StoreError>;
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
pub trait ResourceRepo: Send + Sync {
    async fn upsert<'a>(&self, res: UpsertResource<'a>) -> Result<(), StoreError>;
    async fn list(&self, card_id: i64) -> Result<Vec<ResourceRow>, StoreError>;
}

#[automock]
#[async_trait]
pub trait AgentConfigRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<AgentConfigRow>, StoreError>;
    async fn by_name(&self, name: &str) -> Result<Option<AgentConfigRow>, StoreError>;
    async fn create(&self, cfg: AgentConfigDraft) -> Result<AgentConfigRow, StoreError>;
    async fn update(&self, id: i64, cfg: AgentConfigDraft) -> Result<(), StoreError>;
    async fn remove(&self, id: i64) -> Result<(), StoreError>;
}

#[automock]
#[async_trait]
pub trait SkillRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<SkillRow>, StoreError>;
    async fn create(&self, name: &str, body: &str) -> Result<SkillRow, StoreError>;
    async fn update(&self, id: i64, body: &str) -> Result<(), StoreError>;
    async fn remove(&self, id: i64) -> Result<(), StoreError>;
    async fn list_for_agent(&self, agent_id: i64) -> Result<Vec<SkillRow>, StoreError>;
    async fn attach(&self, agent_id: i64, skill_id: i64) -> Result<(), StoreError>;
    async fn detach(&self, agent_id: i64, skill_id: i64) -> Result<(), StoreError>;
}
