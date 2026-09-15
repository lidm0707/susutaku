//! Outbound ports: interfaces the application core needs from the outside
//! world. Implemented by infrastructure adapters. Traits only — all data
//! types live in the domain layer.
mod agent_git;
mod agent_run;
mod board;
mod chat_memory;
mod inference;
mod model;
mod project_git;
mod runner;
mod search;
mod task;
mod thread_envs;

pub use agent_git::AgentGit;
pub use agent_run::AgentRun;
pub use board::BoardOps;
pub use chat_memory::ChatMemory;
pub use inference::Inference;
pub use model::{ModelEndpoint, ModelEngines, ModelSwitch};
pub use project_git::ProjectGit;
pub use runner::Runner;
pub use search::{Fetcher, Searcher};
pub use task::{
    AgentConfigRepo, CardRepo, CardTx, CommentRepo, CommentTx, MockAgentConfigRepo, MockCardRepo,
    MockCardTx, MockCommentRepo, MockCommentTx, MockProjectRepo, MockResourceRepo, MockSkillRepo,
    MockWorkspaceRepo, ProjectRepo, ResourceRepo, SkillRepo, WorkspaceRepo,
};
pub use thread_envs::ThreadEnvs;
