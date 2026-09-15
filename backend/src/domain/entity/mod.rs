//! Entities: persisted objects, one module per DB table.

pub mod agent_config;
pub mod card;
pub mod project;
pub mod workspace;

pub use agent_config::AgentConfigDraft;
pub use card::{CardMove, CardPatch, NewCard};
pub use project::NewProject;
pub use workspace::NewWorkspace;
