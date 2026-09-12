//! Kanban DB entities: write-side shapes persisted through the repos.

#[derive(Debug, Clone, PartialEq)]
pub struct NewWorkspace {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewProject {
    pub workspace_id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewCard {
    pub project_id: Option<i64>,
    pub column_id: String,
    pub title: String,
    pub description: String,
    pub priority: String,
    /// JSON array of label strings; empty string clears, None unset.
    pub labels: Option<String>,
    /// JSON array of {text, done} objects; empty string clears, None unset.
    pub checklist: Option<String>,
    pub estimate: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardMove {
    pub id: i64,
    pub column_id: String,
    pub position: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardPatch {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub assignee: Option<String>,
    pub deadline: Option<String>,
    pub priority: Option<String>,
    /// None leaves unchanged; empty string clears. Must be a JSON array when set.
    pub labels: Option<String>,
    /// None leaves unchanged; empty string clears. Must be a JSON array when set.
    pub checklist: Option<String>,
    /// Story points; None leaves the current value unchanged.
    pub estimate: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewPipeline {
    pub name: String,
    pub spec: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentConfigDraft {
    pub name: String,
    pub model: String,
    pub persona: String,
    pub prompt: String,
    pub output: String,
    /// Tool allow-list (search|fetch|shell|board); empty = all tools.
    pub allowed_tools: Vec<String>,
}
