//! Kanban board: columns, cards, id-based moves. Pure data + ops, no I/O.

pub mod agent_cfg;
pub mod board;
pub mod card;
pub mod pipeline;
pub mod store;
pub mod user;
pub mod workspace;

pub use agent_cfg::{AgentConfigRow, AgentConfigUpdate};
pub use board::{Board, BoardError, Column};
pub use card::{Card, CardId, Priority, PRIORITY_CRITICAL, PRIORITY_HIGH, PRIORITY_LOW, PRIORITY_NORMAL};
pub use pipeline::PipelineRow;
pub use store::{
    AddCard, AgentState, CardRow, CommentRow, DbTx, MoveCard, Store, StoreError, UpdateCard,
};
pub use user::{NewUser, Role, UserRow, DEFAULT_ADMIN_PASSWORD, DEFAULT_ADMIN_USER, MIN_PASSWORD_LEN};
pub use workspace::{ProjectRow, WorkspaceRow};

pub const DEFAULT_COLUMNS: [(&str, &str); 3] =
    [("todo", "To Do"), ("doing", "Doing"), ("done", "Done")];
