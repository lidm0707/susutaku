//! Board data model: card rows, run ledger types and store-input DTOs.
//! Pure data — the SQL lives in the backend's postgres adapter.

pub const NO_SUCH_CARD_MSG: &str = "no such card";
pub const NO_SUCH_COLUMN_MSG: &str = "no such column";

pub const ACTIVITY_MESSAGE_MAX_CHARS: usize = 500;
pub const ACTIVITY_LIST_MAX: i64 = 200;
pub const ACTIVITY_LIST_DEFAULT: i64 = 50;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{NO_SUCH_CARD_MSG}")]
    NoSuchCard,
    #[error("{NO_SUCH_COLUMN_MSG}")]
    NoSuchColumn,
    #[error("no such workspace")]
    NoSuchWorkspace,
    #[error("no such project")]
    NoSuchProject,
    #[error("workspace name already taken")]
    WorkspaceTaken,
    #[error("project name already taken")]
    ProjectTaken,
    #[error("username already taken")]
    UsernameTaken,
    #[error("agent name already taken")]
    AgentTaken,
    #[error("no such agent")]
    NoSuchAgent,
    #[error("no such skill")]
    NoSuchSkill,
    #[error("skill name already taken")]
    SkillTaken,
    #[error("no such agent output")]
    NoSuchAgentOutput,
    #[error("no such routine")]
    NoSuchRoutine,
    #[error("no such chat thread")]
    NoSuchChatThread,
    #[error("bad spec: {0}")]
    BadSpec(String),
    #[error("password too short")]
    PasswordTooShort,
    #[error("invalid credentials")]
    BadCredentials,
    #[error("unknown role: {0}")]
    BadRole(String),
    #[error("hash error: {0}")]
    Hash(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct CardRow {
    pub id: i64,
    pub column_id: String,
    pub project_id: Option<i64>,
    pub title: String,
    pub description: String,
    pub priority: String,
    pub position: i32,
    pub agent_name: Option<String>,
    pub agent_state: Option<String>,
    /// Last recorded run status (see `RunStatus`); never a runner preference.
    pub run_status: String,
    /// Agent that executed the most recent run; the run *record* role.
    pub last_agent: Option<String>,
    pub last_run_id: Option<i64>,
    pub assignee: Option<String>,
    pub cron: Option<String>,
    pub deadline: Option<String>,
    /// JSON array of label strings; empty string clears, NULL/None unset.
    pub labels: Option<String>,
    /// JSON array of {text, done} objects; empty string clears, NULL/None unset.
    pub checklist: Option<String>,
    pub estimate: Option<i32>,
    /// Per-card image (URL or data URI); NULL/None unset.
    pub image: Option<String>,
}

impl CardRow {
    /// The runner preference: a pinned agent, else human; a run never
    /// mutates what this returns.
    pub fn runner(&self) -> crate::Runner {
        match &self.agent_name {
            Some(agent) => crate::Runner::Pinned(agent.clone()),
            None => crate::Runner::Human,
        }
    }

    pub fn run_status(&self) -> crate::RunStatus {
        crate::RunStatus::parse(&self.run_status)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentState {
    pub name: String,
    #[serde(default)]
    pub state: serde_json::Value,
}

pub struct AddCard<'a> {
    pub project_id: Option<i64>,
    pub column_id: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub priority: &'a str,
    /// JSON array of label strings; empty string clears, None unset.
    pub labels: Option<&'a str>,
    /// JSON array of {text, done} objects; empty string clears, None unset.
    pub checklist: Option<&'a str>,
    pub estimate: Option<i32>,
}

pub struct MoveCard<'a> {
    pub id: i64,
    pub column_id: &'a str,
    pub position: i32,
}

pub struct UpdateCard<'a> {
    pub id: i64,
    pub title: &'a str,
    pub description: &'a str,
    pub assignee: Option<&'a str>,
    /// Empty string clears the deadline; NULL keeps it unset.
    pub deadline: Option<&'a str>,
    /// None leaves the priority unchanged.
    pub priority: Option<&'a str>,
    /// None leaves unchanged; empty string clears. Must be a JSON array when set.
    pub labels: Option<&'a str>,
    /// None leaves unchanged; empty string clears. Must be a JSON array when set.
    pub checklist: Option<&'a str>,
    /// Story points; None leaves the current value unchanged.
    pub estimate: Option<i32>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ActivityRow {
    pub id: i64,
    pub kind: String,
    pub message: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CommentRow {
    pub id: i64,
    pub card_id: i64,
    pub author: String,
    pub body: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RunRecordRow {
    pub id: i64,
    pub card_id: i64,
    pub trigger: String,
    pub agent: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ok: bool,
    pub summary: String,
}

pub struct RunRecordNew {
    pub card_id: i64,
    pub trigger: String,
    pub agent: String,
    pub ok: bool,
    pub summary: String,
}

pub fn validate_card_json(labels: Option<&str>, checklist: Option<&str>) -> Result<(), StoreError> {
    const JSON_ARRAY_ERR: &str = "must be a JSON array";
    for (name, value) in [("labels", labels), ("checklist", checklist)] {
        let Some(text) = value else { continue };
        if text.is_empty() {
            continue;
        }
        let parsed: serde_json::Value = serde_json::from_str(text)
            .map_err(|_| StoreError::BadSpec(format!("{name}: invalid JSON")))?;
        if !parsed.is_array() {
            return Err(StoreError::BadSpec(format!("{name}: {JSON_ARRAY_ERR}")));
        }
    }
    Ok(())
}
