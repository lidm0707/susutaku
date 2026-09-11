//! Domain services: one service per repo port (1:1), plus prompt assembly.
//! Each service holds only its repo trait — composition happens in the app
//! layer.

pub mod agent_config;
pub mod card;
pub mod comment;
pub mod pipeline;
pub mod project;
pub mod prompt;
pub mod resource;
pub mod workspace;

pub use agent_config::AgentConfigService;
pub use card::CardService;
pub use comment::CommentService;
pub use pipeline::PipelineService;
pub use project::ProjectService;
pub use resource::ResourceService;
pub use workspace::WorkspaceService;
