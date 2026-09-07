//! Domain layer: entities, value objects and domain services.
//! No I/O, no framework types.

pub mod entity;
pub mod service;
pub mod valueobject;

pub use entity::tool_call::ToolCall;
pub use service::prompt::{
    Prompt, CONTEXT_FOOTER, CONTEXT_HEADER, CONTEXT_RESULTS_MAX, TOOL_INSTRUCTION,
    TOOL_RESULT_HEADER, TOOL_ROUNDS_MAX,
};
pub use valueobject::search_mode::SearchMode;
pub use valueobject::search_result::SearchResult;
