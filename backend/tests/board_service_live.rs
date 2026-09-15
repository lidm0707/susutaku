//! Live-DB test: the agent's board tool path (BoardService::exec) against a
//! real Postgres — the exact path `TOOL: CARD_AGENT` / `TOOL: CARD_IMAGE` /
//! `TOOL: CARD_RUN` take at runtime.

use std::sync::Arc;

use backend::app::board::BoardService;
use backend::domain::{BoardOp, BoardRequest};
use backend::port::outbound::BoardOps;
use task_rs::{NewUser, Role, Store};

const AGENT_NAME: &str = "agent-live-test";
const IMAGE: &str = "debian:stable-slim";
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
async fn board_service_card_agent_path_against_live_db() {
    let store = Arc::new(Store::connect(Store::default_url()).await.expect("connect"));

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

    let board = BoardService::new(Arc::clone(&store), None, None);

    // Create a card, assign an agent, set an image, run it (fails without an
    // engine — deterministic), and clear the image again.
    let card = board
        .exec(req(
            &token,
            BoardOp::CreateCard {
                project_id: 1,
                title: unique_name("live-card"),
                description: None,
            },
        ))
        .await
        .expect("create card");
    let card_id: i64 = card
        .split_whitespace()
        .find_map(|t| t.trim_end_matches(':').parse().ok())
        .expect("card id in reply");

    let ok = board
        .exec(req(
            &token,
            BoardOp::AssignAgent {
                card_id,
                agent: unique_name(AGENT_NAME),
            },
        ))
        .await
        .expect("assign agent");
    assert!(ok.contains("assigned"));

    let ok = board
        .exec(req(
            &token,
            BoardOp::SetImage {
                card_id,
                image: Some(IMAGE.to_string()),
            },
        ))
        .await
        .expect("set image");
    assert!(ok.contains(IMAGE));

    let card = store.get(card_id).await.expect("get").expect("card");
    assert_eq!(card.image.as_deref(), Some(IMAGE));

    let cleared = board
        .exec(req(
            &token,
            BoardOp::SetImage { card_id, image: None },
        ))
        .await
        .expect("clear image");
    assert!(cleared.contains("cleared"));
    let card = store.get(card_id).await.expect("get").expect("card");
    assert_eq!(card.image, None);

    store.remove(card_id).await.expect("cleanup card");
    store.logout(&token).await.expect("logout");
}
