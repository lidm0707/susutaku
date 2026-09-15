//! Integration test: agent config store against a live Postgres.

use task_rs::{AgentConfigRow, AgentConfigUpdate, Store, StoreError};

const TEST_AGENT: &str = "test-agent";

#[tokio::test]
async fn agent_crud() {
    let store = Store::connect(Store::default_url()).await.expect("connect");
    for row in store.list_agents().await.expect("list agents") {
        if row.name == TEST_AGENT {
            let _ = store.remove_agent(row.id).await;
        }
    }

    let cfg = AgentConfigRow {
        id: 0,
        name: TEST_AGENT.into(),
        model: "qwen3.8-27b-4bit".into(),
        persona: "".into(),
        prompt: "".into(),
        output: "".into(),
        allowed_tools: vec!["search".into(), "board".into()],
        receive_images: true,
        thinking: "off".into(),
    };
    let id = store.create_agent(&cfg).await.expect("create agent");
    assert!(matches!(
        store.create_agent(&cfg).await,
        Err(StoreError::AgentTaken)
    ));

    let rows = store.list_agents().await.expect("list agents");
    let found = rows.iter().find(|r| r.id == id).expect("agent row");
    assert_eq!(found.model, "qwen3.8-27b-4bit");
    assert_eq!(
        found.allowed_tools,
        vec!["search".to_string(), "board".to_string()]
    );
    let by_name = store
        .agent_by_name(TEST_AGENT)
        .await
        .expect("agent_by_name")
        .expect("agent found");
    assert_eq!(by_name.id, id);

    store
        .update_agent(AgentConfigUpdate {
            id,
            name: TEST_AGENT,
            model: "gemma-4-26b-a4b-it-4bit",
            persona: "tester",
            prompt: "hi",
            output: "text",
            allowed_tools: &[],
            receive_images: false,
            thinking: "high",
        })
        .await
        .expect("update agent");
    let cleared = store
        .agent_by_name(TEST_AGENT)
        .await
        .expect("agent_by_name")
        .expect("agent found");
    assert!(cleared.allowed_tools.is_empty());
    assert_eq!(cleared.think_level(), task_rs::ThinkLevel::High);
    assert!(matches!(
        store
            .update_agent(AgentConfigUpdate {
                id: 999_999,
                name: TEST_AGENT,
                model: "",
                persona: "",
                prompt: "",
                output: "",
                allowed_tools: &[],
                receive_images: true,
                thinking: "off",
            })
            .await,
        Err(StoreError::NoSuchAgent)
    ));

    store.remove_agent(id).await.expect("remove agent");
    assert!(matches!(
        store.remove_agent(id).await,
        Err(StoreError::NoSuchAgent)
    ));
}
