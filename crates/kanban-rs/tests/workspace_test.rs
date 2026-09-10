//! Integration test: default workspace + project seeding against a live Postgres.

use kanban_rs::{DEFAULT_PROJECT_NAME, DEFAULT_WORKSPACE_NAME, Store};

#[tokio::test]
async fn default_board_seeded_once_and_idempotent() {
    let store = Store::connect(Store::default_url()).await.expect("connect");

    store.ensure_default_board().await.expect("seed");

    let workspaces = store.list_workspaces().await.expect("list workspaces");
    let seeded = workspaces
        .iter()
        .find(|ws| ws.name == DEFAULT_WORKSPACE_NAME)
        .expect("default workspace exists");
    let projects = store.list_projects(seeded.id).await.expect("list projects");
    assert!(
        projects.iter().any(|p| p.name == DEFAULT_PROJECT_NAME),
        "default project exists"
    );
}
