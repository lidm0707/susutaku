//! Domain layer: entities, value objects and domain services.
//! No I/O, no framework types.

pub mod entity;
pub mod service;

pub mod valueobject;

pub use entity::{AgentConfigDraft, CardMove, CardPatch, NewCard, NewProject, NewWorkspace};
pub use service::prompt::{
    BOARD_TOOL_INSTRUCTION, CONTEXT_FOOTER, CONTEXT_HEADER, CONTEXT_RESULTS_MAX, Prompt,
    TOOL_AGENT_RUN_INSTRUCTION, TOOL_DENIED, TOOL_LSP_INSTRUCTION, TOOL_RESULT_HEADER,
    TOOL_ROUNDS_MAX, TOOL_RULES, TOOL_SHELL_INSTRUCTION, TOOL_WEB_INSTRUCTION,
};
pub use service::{
    AgentConfigService, CardService, CommentService, LspOp, ProjectService, ResourceService,
    SkillService, ToolCall, WorkspaceService,
};
pub use valueobject::TOOL_SUMMARY_MAX;
pub use valueobject::{
    Artifact, ArtifactKind, BoardOp, BoardRequest, BoardResult, BoundRepo, CancelFlag, ChatCmd,
    ChatOutcome, GenReply, GitOp, INTERRUPTED_NOTE, MemoryHit, ReplyRx, SearchMode, SearchResult,
    TOOL_KIND_NAMES, ToolEvent, ToolKind, ToolSet, ToolUse,
};
