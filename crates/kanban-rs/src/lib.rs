//! Kanban board: columns, cards, id-based moves. Pure data + ops, no I/O.

pub mod agent_cfg;
pub mod board;
pub mod card;
pub mod pipeline;
pub mod resource;
pub mod store;
pub mod user;
pub mod workspace;

pub use agent_cfg::{AgentConfigRow, AgentConfigUpdate};
pub use board::{Board, BoardError, Column};
pub use card::{
    Card, CardId, PRIORITY_CRITICAL, PRIORITY_HIGH, PRIORITY_LOW, PRIORITY_NORMAL, Priority,
};
pub use pipeline::PipelineRow;
pub use resource::{ResourceRow, UpsertResource};
pub use store::{
    ACTIVITY_LIST_DEFAULT, ACTIVITY_LIST_MAX, ACTIVITY_MESSAGE_MAX_CHARS, ActivityRow, AddCard,
    AgentOutputRow, AgentState, CardRow, CommentRow, DbTx, MoveCard, OUTPUT_STATUS_APPROVED,
    OUTPUT_STATUS_PENDING, OUTPUT_STATUS_REJECTED, Store, StoreError, UpdateCard,
};
pub use user::{
    DEFAULT_ADMIN_PASSWORD, DEFAULT_ADMIN_USER, MIN_PASSWORD_LEN, NewUser, Role, UserRow,
};
pub use workspace::{DEFAULT_PROJECT_NAME, DEFAULT_WORKSPACE_NAME, ProjectRow, WorkspaceRow};

pub const DEFAULT_COLUMNS: [(&str, &str); 4] = [
    ("todo", "To Do"),
    ("doing", "Doing"),
    ("done", "Done"),
    ("failed", "Failed"),
];
