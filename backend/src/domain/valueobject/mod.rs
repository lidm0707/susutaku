//! Value objects: immutable domain data with no identity.

pub mod board;
pub mod bound_repo;
pub mod chat;
pub mod gen_reply;
pub mod git_op;
pub mod memory_hit;
pub mod search_mode;
pub mod search_result;
pub mod tool_set;
pub mod tool_use;

pub use board::{BoardOp, BoardRequest, BoardResult};
pub use bound_repo::BoundRepo;
pub use chat::{ChatCmd, ChatOutcome};
pub use gen_reply::{GenReply, ReplyRx};
pub use git_op::GitOp;
pub use memory_hit::MemoryHit;
pub use search_mode::SearchMode;
pub use search_result::SearchResult;
pub use tool_set::{TOOL_KIND_NAMES, ToolKind, ToolSet};
pub use tool_use::{TOOL_SUMMARY_MAX, ToolUse};
