//! Workspaces and projects: a workspace groups projects, projects group tasks.

use serde::Serialize;

pub const DEFAULT_WORKSPACE_NAME: &str = "workspace";
pub const DEFAULT_PROJECT_NAME: &str = "project";

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct WorkspaceRow {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProjectRow {
    pub id: i64,
    pub workspace_id: i64,
    pub name: String,
}
