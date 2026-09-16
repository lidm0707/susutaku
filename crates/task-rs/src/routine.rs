use serde::{Deserialize, Serialize};

pub type RoutineId = u64;

/// Run triggers for `routine_runs.trigger`.
pub const ROUTINE_TRIGGER_MANUAL: &str = "manual";
pub const ROUTINE_TRIGGER_CRON: &str = "cron";

/// Who may see/handle routines: the owner role only (API enforces).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineRow {
    pub id: i64,
    pub name: String,
    /// 5-field UTC cron expression.
    pub cron: String,
    /// Agent whose persona/model runs the routine; empty = default engine.
    pub agent: String,
    /// What the routine does each run.
    pub instruction: String,
    /// Disabled routines stay listed but never schedule.
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineRunRow {
    pub id: i64,
    pub routine_id: i64,
    /// "manual" | "cron".
    pub trigger: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub ok: bool,
    pub summary: String,
}

/// Body of a create/update routine request (backend API reuses this shape).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct RoutineDraft {
    pub name: String,
    pub cron: String,
    pub agent: String,
    pub instruction: String,
    pub enabled: bool,
}
