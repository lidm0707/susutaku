//! Outbound ports: interfaces the application core needs from the outside
//! world. Implemented by infrastructure adapters. Traits only — all data
//! types live in the domain layer.
mod board;
mod chat_memory;
mod inference;
mod kanban;
mod model;
mod runner;
mod search;

pub use board::BoardOps;
pub use chat_memory::ChatMemory;
pub use inference::Inference;
pub use kanban::{
    AgentConfigRepo, CardRepo, CardTx, CommentRepo, CommentTx, MockAgentConfigRepo, MockCardRepo,
    MockCardTx, MockCommentRepo, MockCommentTx, MockPipelineRepo, MockPipelineTx, MockProjectRepo,
    MockResourceRepo, MockWorkspaceRepo, PipelineRepo, PipelineTx, ProjectRepo, ResourceRepo,
    WorkspaceRepo,
};
pub use model::{ModelEndpoint, ModelSwitch};
pub use runner::Runner;
pub use search::{Fetcher, Searcher};
