//! Integration test: Store::running_cards against a live Postgres — only
//! cards in the queued/running run states show up as in-flight.

use task_rs::{AddCard, RunRecordNew};

use backend::infra::postgres::Store;

const TEST_TITLE: &str = "active-runs test card";

#[tokio::test]
async fn running_cards_lists_only_in_flight() {
    let store = Store::connect(Store::default_url()).await.expect("connect");

    let id = store
        .add(AddCard {
            project_id: None,
            column_id: task_rs::COLUMN_TODO,
            title: TEST_TITLE,
            description: "",
            priority: task_rs::PRIORITY_NORMAL,
            labels: None,
            checklist: None,
            estimate: None,
        })
        .await
        .expect("add");

    // Idle card: must NOT appear.
    assert!(store.running_cards().await.expect("running").is_empty());

    store
        .set_run_start(id, "qa-agent")
        .await
        .expect("run start");
    let running = store.running_cards().await.expect("running");
    let row = running.iter().find(|c| c.id == id).expect("card in flight");
    assert_eq!(row.run_status, task_rs::RUN_STATUS_RUNNING);
    assert_eq!(row.last_agent.as_deref(), Some("qa-agent"));

    // A finished run moves the card out of the in-flight set.
    store
        .record_run(RunRecordNew {
            card_id: id,
            trigger: task_rs::TRIGGER_MANUAL.to_owned(),
            agent: "qa-agent".to_owned(),
            ok: true,
            summary: "done".to_owned(),
        })
        .await
        .expect("record run");
    assert!(
        store
            .running_cards()
            .await
            .expect("running")
            .iter()
            .all(|c| c.id != id)
    );

    store.remove(id).await.expect("remove");
}
