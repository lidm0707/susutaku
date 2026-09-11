//! Domain layer: entities, value objects and domain services.
//! No I/O, no framework types.

pub mod entity;
pub mod service;
pub mod valueobject;

pub use entity::{
    AgentConfigDraft, CardMove, CardPatch, NewCard, NewPipeline, NewProject, NewWorkspace, ToolCall,
};
pub use service::prompt::{
    CONTEXT_FOOTER, CONTEXT_HEADER, CONTEXT_RESULTS_MAX, Prompt, TOOL_INSTRUCTION,
    TOOL_RESULT_HEADER, TOOL_ROUNDS_MAX,
};
pub use service::{
    AgentConfigService, CardService, CommentService, PipelineService, ProjectService,
    ResourceService, WorkspaceService,
};
pub use valueobject::{
    BoardOp, BoardRequest, BoardResult, ChatCmd, ChatOutcome, GenReply, MemoryHit, ReplyRx,
    SearchMode, SearchResult,
};
