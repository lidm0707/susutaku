//! Live-DB test: the agent's board tool path (BoardService::exec) against a
//! real Postgres — the exact path `TOOL: PIPELINE_CREATE` takes at runtime.
//! A spec missing a required stage param must be rejected; a valid one stored.

use std::sync::Arc;

use backend::app::board::BoardService;
use backend::domain::{BoardOp, BoardRequest};
use backend::port::outbound::BoardOps;
use kanban_rs::{NewUser, Role, Store};

const PIPE_NAME: &str = "agent-live-test";
const SPEC_AGENT_OK: &str = r#"{"nodes":[{"id":"a","stage":"search","params":{"query":"trump"}},{"id":"b","stage":"agent","params":{"agent":"zai"}}],"links":[{"from":"a","to":"b"}]}"#;
const SPEC_AGENT_BAD: &str = r#"{"nodes":[{"id":"b","stage":"agent"}],"links":[]}"#;
const PASSWORD: &str = "s3cret-password";

fn unique_name(tag: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{tag}_{nanos}")
}

fn req(token: &str, op: BoardOp) -> BoardRequest {
    BoardRequest {
        token: Some(token.to_string()),
        op,
    }
}

#[tokio::test]
async fn board_service_pipeline_path_against_live_db() {
    let store = Arc::new(Store::connect(Store::default_url()).await.expect("connect"));
    for p in store.list_pipelines().await.expect("list") {
        if p.name == PIPE_NAME {
            let _ = store.remove_pipeline(p.id).await;
        }
    }

    let username = unique_name("agent_board_user");
    store
        .create_user(&NewUser {
            username: &username,
            password: PASSWORD,
            role: Role::Editor,
        })
        .await
        .expect("create user");
    let token = store
        .login(&username, PASSWORD)
        .await
        .expect("login")
        .expect("token");

    let board = BoardService::new(Arc::clone(&store));

    // Invalid spec (agent node without required param) must be rejected.
    let bad = board
        .exec(req(
            &token,
            BoardOp::CreatePipeline {
                name: PIPE_NAME.to_string(),
                spec: Some(SPEC_AGENT_BAD.to_string()),
            },
        ))
        .await;
    assert!(bad.is_err(), "missing required param must fail validation");

    // Valid spec goes through, and the stored spec round-trips.
    let ok = board
        .exec(req(
            &token,
            BoardOp::CreatePipeline {
                name: PIPE_NAME.to_string(),
                spec: Some(SPEC_AGENT_OK.to_string()),
            },
        ))
        .await
        .expect("create pipeline");
    assert!(ok.starts_with("pipeline "));

    let rows = store.list_pipelines().await.expect("list");
    let row = rows
        .iter()
        .find(|p| p.name == PIPE_NAME)
        .expect("pipeline stored");
    assert_eq!(row.spec, SPEC_AGENT_OK);

    // No spec -> valid empty default (this was the broken `{"stages": []}`).
    let empty_name = unique_name("agent_board_empty");
    let ok = board
        .exec(req(
            &token,
            BoardOp::CreatePipeline {
                name: empty_name.clone(),
                spec: None,
            },
        ))
        .await
        .expect("create empty pipeline");
    assert!(ok.starts_with("pipeline "));
    let rows = store.list_pipelines().await.expect("list");
    let row = rows
        .iter()
        .find(|p| p.name == empty_name)
        .expect("empty pipeline stored");
    assert_eq!(row.spec, r#"{"nodes":[],"links":[]}"#);

    // Cleanup.
    for p in store.list_pipelines().await.expect("list") {
        if p.name == PIPE_NAME || p.name == empty_name {
            let _ = store.remove_pipeline(p.id).await;
        }
    }
    store.logout(&token).await.expect("logout");
}
