//! Integration test: workspaces > projects > tasks against a live Postgres.

use kanban_rs::{AddCard, Store};

fn unique(tag: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{tag}_{nanos}")
}

#[tokio::test]
async fn workspace_project_task_roundtrip() {
    let store = Store::connect(Store::default_url()).await.expect("connect");

    let ws = store.create_workspace(&unique("ws")).await.expect("workspace");
    let project = store
        .create_project(ws.id, &unique("proj"))
        .await
        .expect("project");

    let projects = store.list_projects(ws.id).await.expect("list projects");
    assert!(projects.iter().any(|p| p.id == project.id));

    // Duplicate project name in the same workspace is rejected.
    let dup_name = format!("dup_{}", nanos());
    let _ = store.create_project(ws.id, &dup_name).await.expect("first");
    assert!(store.create_project(ws.id, &dup_name).await.is_err());

    // Task created inside the project, listed through the project filter.
    let task_id = store
        .add(AddCard {
            project_id: Some(project.id),
            column_id: "todo",
            title: "a task",
            description: "",
            priority: "normal",
        })
        .await
        .expect("add task");

    let in_project = store
        .list(Some(project.id))
        .await
        .expect("list by project");
    assert!(in_project.iter().any(|c| c.id == task_id));
    assert!(in_project.iter().all(|c| c.project_id == Some(project.id)));

    // Deleting the workspace cascades to project and tasks.
    store.delete_workspace(ws.id).await.expect("delete ws");
    assert!(store.delete_workspace(ws.id).await.is_err());
    assert!(store.delete_project(project.id).await.is_err());
    let left = store.list(Some(project.id)).await.expect("list");
    assert!(left.is_empty());
}

fn nanos() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}
