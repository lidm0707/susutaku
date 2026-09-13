//! Workspace table entity: write-side shape for `workspaces`.

#[derive(Debug, Clone, PartialEq)]
pub struct NewWorkspace {
    pub name: String,
}
