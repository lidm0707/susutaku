//! Workspaces and projects: a workspace groups projects, projects group tasks.

use serde::Serialize;

use crate::store::{Store, StoreError};

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

impl Store {
    /// Seed one workspace + one project on first boot; no-op once any
    /// workspace exists (both are renameable via the API).
    pub async fn ensure_default_board(&self) -> Result<(), StoreError> {
        if !self.list_workspaces().await?.is_empty() {
            return Ok(());
        }
        let ws = self.create_workspace(DEFAULT_WORKSPACE_NAME).await?;
        self.create_project(ws.id, DEFAULT_PROJECT_NAME).await?;
        Ok(())
    }

    pub async fn create_workspace(&self, name: &str) -> Result<WorkspaceRow, StoreError> {
        let row = sqlx::query_as!(
            WorkspaceRow,
            r#"INSERT INTO workspaces (name) VALUES ($1)
               RETURNING id, name"#,
            name,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e.as_database_error() {
            Some(d) if d.code().as_deref() == Some("23505") => StoreError::WorkspaceTaken,
            _ => StoreError::Db(e),
        })?;
        Ok(row)
    }

    pub async fn list_workspaces(&self) -> Result<Vec<WorkspaceRow>, StoreError> {
        let rows = sqlx::query_as!(
            WorkspaceRow,
            r#"SELECT id, name FROM workspaces ORDER BY id"#
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn delete_workspace(&self, id: i64) -> Result<(), StoreError> {
        let res = sqlx::query!(r#"DELETE FROM workspaces WHERE id = $1"#, id)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchWorkspace);
        }
        Ok(())
    }

    pub async fn create_project(
        &self,
        workspace_id: i64,
        name: &str,
    ) -> Result<ProjectRow, StoreError> {
        let row = sqlx::query_as!(
            ProjectRow,
            r#"INSERT INTO projects (workspace_id, name) VALUES ($1, $2)
               RETURNING id, workspace_id, name"#,
            workspace_id,
            name,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e.as_database_error() {
            Some(d) if d.code().as_deref() == Some("23505") => StoreError::ProjectTaken,
            _ => StoreError::Db(e),
        })?;
        Ok(row)
    }

    pub async fn list_projects(&self, workspace_id: i64) -> Result<Vec<ProjectRow>, StoreError> {
        let rows = sqlx::query_as!(
            ProjectRow,
            r#"SELECT id, workspace_id, name FROM projects
               WHERE workspace_id = $1 ORDER BY id"#,
            workspace_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn delete_project(&self, id: i64) -> Result<(), StoreError> {
        let res = sqlx::query!(r#"DELETE FROM projects WHERE id = $1"#, id)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchProject);
        }
        Ok(())
    }
}
