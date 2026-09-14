//! Run-ticket lifecycle: issue -> resolve -> consume -> gone.

use kanban_rs::{NewAgentRunToken, Store, TOKEN_PREFIX};

#[tokio::test]
async fn agent_token_roundtrip() {
    let store = Store::connect(&format!(
        "postgres://susutaku:susutaku@localhost:5434/{}",
        std::env::var("TEST_DB_NAME").unwrap_or_else(|_| "susutaku".into())
    ))
    .await
    .expect("connect");

    store
        .prune_expired_agent_tokens()
        .await
        .expect("prune before");

    let issued = store
        .issue_agent_token(&NewAgentRunToken {
            agent: "zai",
            project_id: None,
            card_id: None,
            thread_id: None,
            machine: "test-host",
        })
        .await
        .expect("issue");
    assert!(issued.token.starts_with(TOKEN_PREFIX));
    assert_eq!(issued.row.agent, "zai");
    assert_eq!(issued.row.machine, "test-host");

    let resolved = store
        .resolve_agent_token(&issued.token)
        .await
        .expect("resolve")
        .expect("ticket live");
    assert_eq!(resolved.token_hash, issued.row.token_hash);

    let active = store.list_active_agent_tokens().await.expect("active");
    assert!(active.iter().any(|r| r.agent == "zai"));

    store
        .consume_agent_token(&issued.row.token_hash)
        .await
        .expect("consume");

    assert!(
        store
            .resolve_agent_token(&issued.token)
            .await
            .expect("resolve after consume")
            .is_none()
    );
}

#[tokio::test]
async fn bad_token_is_rejected() {
    let store = Store::connect(&format!(
        "postgres://susutaku:susutaku@localhost:5434/{}",
        std::env::var("TEST_DB_NAME").unwrap_or_else(|_| "susutaku".into())
    ))
    .await
    .expect("connect");

    assert!(
        store
            .resolve_agent_token("ag_does_not_exist")
            .await
            .expect("resolve")
            .is_none()
    );
}
