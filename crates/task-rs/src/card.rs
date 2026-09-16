use serde::{Deserialize, Serialize};

pub type CardId = u64;

pub const MIN_PRIORITY: u8 = 0;
pub const MAX_PRIORITY: u8 = 9;
pub const DEFAULT_PRIORITY: u8 = 5;

pub const PRIORITY_LOW: &str = "low";
pub const PRIORITY_NORMAL: &str = "normal";
pub const PRIORITY_HIGH: &str = "high";
pub const PRIORITY_CRITICAL: &str = "critical";

/// Run-status values persisted on `task_cards.run_status`.
pub const RUN_STATUS_IDLE: &str = "idle";
pub const RUN_STATUS_QUEUED: &str = "queued";
pub const RUN_STATUS_RUNNING: &str = "running";
pub const RUN_STATUS_FINISHED: &str = "finished";
pub const RUN_STATUS_ERROR: &str = "error";

/// What fired a run; recorded on every `run_records` row.
pub const TRIGGER_MANUAL: &str = "manual";
pub const TRIGGER_CRON: &str = "cron";

/// Runner preference kinds, as exposed by the API.
pub const RUNNER_HUMAN: &str = "human";
pub const RUNNER_PINNED: &str = "pinned";

/// Canonical workflow status of a Task (= Board Card column).
pub const STATUS_TODO: &str = "todo";
pub const STATUS_IN_PROGRESS: &str = "in_progress";
pub const STATUS_REVIEW: &str = "review";
pub const STATUS_CONFLICT: &str = "conflict";
pub const STATUS_DONE: &str = "done";
pub const STATUS_FAILED: &str = "failed";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Todo,
    InProgress,
    Review,
    Conflict,
    Done,
    Failed,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Todo => STATUS_TODO,
            TaskStatus::InProgress => STATUS_IN_PROGRESS,
            TaskStatus::Review => STATUS_REVIEW,
            TaskStatus::Conflict => STATUS_CONFLICT,
            TaskStatus::Done => STATUS_DONE,
            TaskStatus::Failed => STATUS_FAILED,
        }
    }

    /// Parse from a status string or a legacy column id (`doing` = in progress).
    pub fn parse(raw: &str) -> Self {
        match raw {
            STATUS_IN_PROGRESS | crate::COLUMN_DOING => TaskStatus::InProgress,
            STATUS_REVIEW => TaskStatus::Review,
            STATUS_CONFLICT => TaskStatus::Conflict,
            STATUS_DONE => TaskStatus::Done,
            STATUS_FAILED => TaskStatus::Failed,
            _ => TaskStatus::Todo,
        }
    }

    /// The board column this status is stored as (`column_id`).
    pub fn column(self) -> &'static str {
        match self {
            TaskStatus::Todo => crate::COLUMN_TODO,
            TaskStatus::InProgress => crate::COLUMN_DOING,
            TaskStatus::Review => STATUS_REVIEW,
            TaskStatus::Conflict => STATUS_CONFLICT,
            TaskStatus::Done => crate::COLUMN_DONE,
            TaskStatus::Failed => crate::COLUMN_FAILED,
        }
    }
}

/// Server-side transition authority for `PATCH /api/tasks/{id}/status`.
pub fn transition_allowed(from: TaskStatus, to: TaskStatus) -> bool {
    use TaskStatus::*;
    match from {
        Todo => matches!(to, InProgress | Done | Failed),
        InProgress => matches!(to, Review | Conflict | Done | Failed | Todo),
        Review => matches!(to, Done | Conflict | InProgress),
        Conflict => matches!(to, Review | InProgress | Done),
        Done => matches!(to, Todo | InProgress),
        Failed => matches!(to, Todo | InProgress),
    }
}

/// Who runs a card: a preference, never mutated by a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Runner {
    Human,
    /// A specific agent is pinned to this card.
    Pinned(String),
}

impl Runner {
    pub fn as_kind(&self) -> &'static str {
        match self {
            Runner::Human => RUNNER_HUMAN,
            Runner::Pinned(_) => RUNNER_PINNED,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    Idle,
    Queued,
    Running,
    Finished,
    Error,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Idle => RUN_STATUS_IDLE,
            RunStatus::Queued => RUN_STATUS_QUEUED,
            RunStatus::Running => RUN_STATUS_RUNNING,
            RunStatus::Finished => RUN_STATUS_FINISHED,
            RunStatus::Error => RUN_STATUS_ERROR,
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            RUN_STATUS_QUEUED => RunStatus::Queued,
            RUN_STATUS_RUNNING => RunStatus::Running,
            RUN_STATUS_FINISHED => RunStatus::Finished,
            RUN_STATUS_ERROR => RunStatus::Error,
            _ => RunStatus::Idle,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

impl Priority {
    pub fn level(self) -> u8 {
        match self {
            Priority::Low => 1,
            Priority::Normal => DEFAULT_PRIORITY,
            Priority::High => 8,
            Priority::Critical => MAX_PRIORITY,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    pub id: CardId,
    pub title: String,
    pub description: String,
    pub priority: Priority,
}

impl Card {
    pub fn new(id: CardId, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            description: String::new(),
            priority: Priority::Normal,
        }
    }
}
