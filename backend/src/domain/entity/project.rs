//! Project table entity: write-side shape for `projects`.

#[derive(Debug, Clone, PartialEq)]
pub struct NewProject {
    pub workspace_id: i64,
    pub name: String,
}
