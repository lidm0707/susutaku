//! Card table entities: write-side shapes for `cards`.

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
