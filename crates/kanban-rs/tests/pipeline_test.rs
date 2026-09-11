//! Integration test: pipeline + agent config store against a live Postgres.

use kanban_rs::{AddCard, AgentConfigRow, AgentConfigUpdate, PipelineRow, Store, StoreError};

const TEST_PIPELINE_A: &str = "test-pipeline-a";
const TEST_PIPELINE_B: &str = "test-pipeline-b";
const TEST_AGENT: &str = "test-agent";
const TEST_COLUMN: &str = "todo";
const TEST_TITLE: &str = "pipeline test card";
const TEST_PRIORITY: &str = "normal";
const VALID_SPEC: &str = r#"{"nodes":[{"id":"a","stage":"ingest","params":null}],"links":[]}"#;
const BAD_SPEC: &str = r#"{"nodes":[],"links":[]}"#;

#[tokio::test]
async fn pipeline_crud_and_validation() {
    let store = Store::connect(Store::default_url()).await.expect("connect");

    let id = store
        .create_pipeline(TEST_PIPELINE_A, VALID_SPEC)
        .await
        .expect("create");
    assert!(matches!(
        store.create_pipeline(TEST_PIPELINE_A, VALID_SPEC).await,
        Err(StoreError::PipelineTaken)
    ));
    assert!(matches!(
        store.create_pipeline(TEST_PIPELINE_B, BAD_SPEC).await,
        Err(StoreError::BadSpec(_))
    ));

    let rows = store.list_pipelines().await.expect("list");
    assert!(
        rows.iter()
            .any(|PipelineRow { name, spec, .. }| name == TEST_PIPELINE_A && spec == VALID_SPEC)
    );

    store
        .update_pipeline(id, TEST_PIPELINE_B, VALID_SPEC)
        .await
        .expect("update");
    assert!(matches!(
        store
            .update_pipeline(999_999, TEST_PIPELINE_B, VALID_SPEC)
            .await,
        Err(StoreError::NoSuchPipeline)
    ));

    store.remove_pipeline(id).await.expect("remove");
    assert!(matches!(
        store.remove_pipeline(id).await,
        Err(StoreError::NoSuchPipeline)
    ));
}

#[tokio::test]
async fn agent_crud_and_card_link() {
    let store = Store::connect(Store::default_url()).await.expect("connect");

    let cfg = AgentConfigRow {
        id: 0,
        name: TEST_AGENT.into(),
        model: "qwen3.8-27b-4bit".into(),
        persona: "".into(),
        prompt: "".into(),
        output: "".into(),
    };
    let id = store.create_agent(&cfg).await.expect("create agent");
    assert!(matches!(
        store.create_agent(&cfg).await,
        Err(StoreError::AgentTaken)
    ));

    let rows = store.list_agents().await.expect("list agents");
    let found = rows.iter().find(|r| r.id == id).expect("agent row");
    assert_eq!(found.model, "qwen3.8-27b-4bit");

    store
        .update_agent(AgentConfigUpdate {
            id,
            name: TEST_AGENT,
            model: "gemma-4-26b-a4b-it-4bit",
            persona: "tester",
            prompt: "hi",
            output: "text",
        })
        .await
        .expect("update agent");
    assert!(matches!(
        store
            .update_agent(AgentConfigUpdate {
                id: 999_999,
                name: TEST_AGENT,
                model: "",
                persona: "",
                prompt: "",
                output: "",
            })
            .await,
        Err(StoreError::NoSuchAgent)
    ));

    let card_id = store
        .add(AddCard {
            project_id: None,
            column_id: TEST_COLUMN,
            title: TEST_TITLE,
            description: "",
            priority: TEST_PRIORITY,
            labels: None,
            checklist: None,
            estimate: None,
        })
        .await
        .expect("add card");
    let pipe_id = store
        .create_pipeline(TEST_PIPELINE_A, VALID_SPEC)
        .await
        .expect("create pipeline for link");
    store
        .set_card_pipeline(card_id, Some(pipe_id))
        .await
        .expect("link");
    let card = store.get(card_id).await.expect("get").expect("card");
    assert_eq!(card.pipeline_id, Some(pipe_id));
    assert!(matches!(
        store.set_card_pipeline(999_999, None).await,
        Err(StoreError::NoSuchCard)
    ));

    store.remove(card_id).await.expect("remove card");
    store
        .remove_pipeline(pipe_id)
        .await
        .expect("remove pipeline");
    store.remove_agent(id).await.expect("remove agent");
    assert!(matches!(
        store.remove_agent(id).await,
        Err(StoreError::NoSuchAgent)
    ));
}
