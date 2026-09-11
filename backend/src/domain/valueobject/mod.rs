//! Value objects: immutable domain data with no identity.

pub mod board;
pub mod chat;
pub mod gen_reply;
pub mod memory_hit;
pub mod search_mode;
pub mod search_result;

pub use board::{BoardOp, BoardRequest, BoardResult};
pub use chat::{ChatCmd, ChatOutcome};
pub use gen_reply::{GenReply, ReplyRx};
pub use memory_hit::MemoryHit;
pub use search_mode::SearchMode;
pub use search_result::SearchResult;
