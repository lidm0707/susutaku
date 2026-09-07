//! Outbound ports: interfaces the application core needs from the outside
//! world. Implemented by infrastructure adapters.

mod inference;
mod kanban;
mod model;
mod runner;
mod search;

pub use inference::{GenReply, Inference, ReplyRx};
pub use kanban::{
    AgentConfigDraft, AgentConfigRepo, CardMove, CardPatch, CardRepo, CardTx, CommentRepo,
    CommentTx, MockAgentConfigRepo, MockCardRepo, MockCardTx, MockCommentRepo, MockCommentTx,
    MockPipelineRepo, MockPipelineTx, MockProjectRepo, MockWorkspaceRepo, NewCard, NewPipeline,
    NewProject, NewWorkspace, PipelineRepo, PipelineTx, ProjectRepo, WorkspaceRepo,
};
pub use model::ModelSwitch;
pub use runner::Runner;
pub use search::{Fetcher, Searcher};
