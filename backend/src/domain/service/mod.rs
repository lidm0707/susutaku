//! Domain services: one service per repo port (1:1), plus prompt assembly.
//! Each service holds only its repo trait — composition happens in the app
//! layer.

pub mod agent_config;
pub mod card;
pub mod comment;
pub mod project;
pub mod prompt;
pub mod resource;
pub mod skill;
pub mod tool_call;
pub mod workspace;

pub use agent_config::AgentConfigService;
pub use card::CardService;
pub use comment::CommentService;
pub use project::ProjectService;
pub use resource::ResourceService;
pub use skill::SkillService;
pub use tool_call::{LspOp, ToolCall};
pub use workspace::WorkspaceService;
