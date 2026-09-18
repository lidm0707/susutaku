//! Integration test: Store against a live Postgres (DATABASE_URL or default).


use task_rs::{AddCard, AgentState, MoveCard};
use backend::infra::postgres::Store;

const TEST_COLUMN_A: &str = "todo";
const TEST_COLUMN_B: &str = "doing";
const TEST_TITLE: &str = "test card";
const TEST_PRIORITY: &str = "high";
const TEST_AGENT_NAME: &str = "qwen-agent";
const TEST_IMAGE: &str = "https://example.com/img.png";

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
            labels: None,
            checklist: None,
            estimate: None,
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
        .set_agent(
            id,
            &AgentState {
                name: TEST_AGENT_NAME.into(),
                state: state.clone(),
            },
        )
        .await
        .expect("set agent");
    let agent = store.agent(id).await.expect("agent").expect("agent set");
    assert_eq!(agent.name, TEST_AGENT_NAME);
    assert_eq!(agent.state, state);

    assert_eq!(store.get(id).await.expect("get").unwrap().image, None);
    store
        .set_card_image(id, Some(TEST_IMAGE))
        .await
        .expect("set image");
    let row = store.get(id).await.expect("get").expect("row exists");
    assert_eq!(row.image.as_deref(), Some(TEST_IMAGE));
    store.set_card_image(id, None).await.expect("clear image");
    assert_eq!(store.get(id).await.expect("get").unwrap().image, None);
    assert!(matches!(
        store.set_card_image(999_999, None).await,
        Err(task_rs::StoreError::NoSuchCard)
    ));

    store.remove(id).await.expect("remove");
    assert!(store.get(id).await.expect("get").is_none());
    assert!(store.remove(id).await.is_err());
}
