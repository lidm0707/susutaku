//! Integration test: Store against a live Postgres (DATABASE_URL or default).

use kanban_rs::{AddCard, AgentState, MoveCard, Store};

const TEST_COLUMN_A: &str = "todo";
const TEST_COLUMN_B: &str = "doing";
const TEST_TITLE: &str = "test card";
const TEST_PRIORITY: &str = "high";
const TEST_AGENT_NAME: &str = "qwen-agent";

#[tokio::test]
async fn card_and_agent_roundtrip() {
    let store = Store::connect(Store::default_url()).await.expect("connect");

    let id = store
        .add(AddCard {
            project_id: None,
            column_id: TEST_COLUMN_A,
            title: TEST_TITLE,
            description: "",
            priority: TEST_PRIORITY,
        })
        .await
        .expect("add");

    let row = store.get(id).await.expect("get").expect("row exists");
    assert_eq!(row.title, TEST_TITLE);
    assert_eq!(row.priority, TEST_PRIORITY);

    store
        .move_card(MoveCard {
            id,
            column_id: TEST_COLUMN_B,
            position: 0,
        })
        .await
        .expect("move");
    let row = store.get(id).await.expect("get").expect("row exists");
    assert_eq!(row.column_id, TEST_COLUMN_B);

    let state = serde_json::json!({ "step": 2, "done": false });
    store
        .set_agent(id, &AgentState { name: TEST_AGENT_NAME.into(), state: state.clone() })
        .await
        .expect("set agent");
    let agent = store.agent(id).await.expect("agent").expect("agent set");
    assert_eq!(agent.name, TEST_AGENT_NAME);
    assert_eq!(agent.state, state);

    store.remove(id).await.expect("remove");
    assert!(store.get(id).await.expect("get").is_none());
    assert!(store.remove(id).await.is_err());
}
