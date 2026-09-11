//! Entities: persisted and use-case objects.

pub mod kanban;
pub mod tool_call;

pub use kanban::{
    AgentConfigDraft, CardMove, CardPatch, NewCard, NewPipeline, NewProject, NewWorkspace,
};
pub use tool_call::ToolCall;
