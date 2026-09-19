//! Task board: columns, cards, id-based moves. Pure data + ops, no I/O.
//! Persistence (SQL) lives in the backend's postgres adapter.

pub mod agent_cfg;
pub mod agent_token;
pub mod attachment;
pub mod board;
pub mod card;
pub mod chat;
pub mod resource;
pub mod routine;
pub mod skill;
pub mod store;
pub mod user;
pub mod workspace;

pub use agent_cfg::{AgentConfigRow, AgentConfigUpdate, CtxPolicy, ThinkLevel};
pub use agent_token::{
    ActiveAgentRun, AgentRunTokenRow, IssuedAgentToken, NewAgentRunToken, TOKEN_BYTES,
    TOKEN_PREFIX, TOKEN_TTL_SECS,
};
pub use attachment::AttachmentLinkRow;
pub use board::{Board, BoardError, Column};
pub use card::{
    Card, CardId, PRIORITY_CRITICAL, PRIORITY_HIGH, PRIORITY_LOW, PRIORITY_NORMAL, Priority,
    RUN_STATUS_ERROR, RUN_STATUS_FINISHED, RUN_STATUS_IDLE, RUN_STATUS_QUEUED, RUN_STATUS_RUNNING,
    RUNNER_HUMAN, RUNNER_PINNED, RunStatus, Runner, STATUS_CONFLICT, STATUS_DONE, STATUS_FAILED,
    STATUS_IN_PROGRESS, STATUS_REVIEW, STATUS_TODO, TRIGGER_CRON, TRIGGER_MANUAL, TaskStatus,
    transition_allowed,
};
pub use chat::{ChatMessageRow, ChatThreadRow, ROLE_ASSISTANT, ROLE_USER};
pub use resource::{ResourceRow, UpsertResource};
pub use routine::{
    ROUTINE_TRIGGER_CRON, ROUTINE_TRIGGER_MANUAL, RoutineDraft, RoutineId, RoutineRow,
    RoutineRunRow,
};
pub use skill::{NewSkill, SkillRow};
pub use store::{
    ACTIVITY_LIST_DEFAULT, ACTIVITY_LIST_MAX, ACTIVITY_MESSAGE_MAX_CHARS, ActivityRow, AddCard,
    AgentState, CardRow, CommentRow, MoveCard, RunRecordNew, RunRecordRow, StoreError, UpdateCard,
    validate_card_json,
};
pub use user::{
    DEFAULT_ADMIN_PASSWORD, DEFAULT_ADMIN_USER, MIN_PASSWORD_LEN, NewUser, Role, UserRow,
    hash_password, verify_password,
};
pub use workspace::{DEFAULT_PROJECT_NAME, DEFAULT_WORKSPACE_NAME, ProjectRow, WorkspaceRow};

pub const COLUMN_TODO: &str = "todo";
pub const COLUMN_DOING: &str = "doing";
pub const COLUMN_DONE: &str = "done";
pub const COLUMN_FAILED: &str = "failed";

pub const DEFAULT_COLUMNS: [(&str, &str); 4] = [
    (COLUMN_TODO, "To Do"),
    (COLUMN_DOING, "Doing"),
    (COLUMN_DONE, "Done"),
    (COLUMN_FAILED, "Failed"),
];
