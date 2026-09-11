//! Integration test: card labels/checklist/estimate round-trip and validation.

use kanban_rs::{AddCard, Store, StoreError, UpdateCard};

const TEST_COLUMN: &str = "todo";
const TEST_TITLE: &str = "labels card";
const TEST_PRIORITY: &str = "normal";
const LABELS_A: &str = r#"["bug","infra"]"#;
const LABELS_B: &str = r#"["feature"]"#;
const CHECKLIST: &str = r#"[{"text":"write test","done":false},{"text":"ship","done":true}]"#;
const ESTIMATE: i32 = 5;

#[tokio::test]
async fn card_labels_checklist_estimate_roundtrip() {
    let store = Store::connect(Store::default_url()).await.expect("connect");

    let id = store
        .add(AddCard {
            project_id: None,
            column_id: TEST_COLUMN,
            title: TEST_TITLE,
            description: "",
            priority: TEST_PRIORITY,
            labels: Some(LABELS_A),
            checklist: Some(CHECKLIST),
            estimate: Some(ESTIMATE),
        })
        .await
        .expect("add");

    let row = store.get(id).await.expect("get").expect("row exists");
    assert_eq!(row.labels.as_deref(), Some(LABELS_A));
    assert_eq!(row.checklist.as_deref(), Some(CHECKLIST));
    assert_eq!(row.estimate, Some(ESTIMATE));

    let listed = store
        .list(None)
        .await
        .expect("list")
        .into_iter()
        .find(|c| c.id == id)
        .expect("listed");
    assert_eq!(listed.labels.as_deref(), Some(LABELS_A));

    store
        .update_card(UpdateCard {
            id,
            title: TEST_TITLE,
            description: "",
            assignee: None,
            deadline: None,
            priority: None,
            labels: Some(LABELS_B),
            checklist: None,
            estimate: None,
        })
        .await
        .expect("update labels");

    let row = store.get(id).await.expect("get").expect("row exists");
    assert_eq!(row.labels.as_deref(), Some(LABELS_B));
    assert_eq!(row.checklist.as_deref(), Some(CHECKLIST));
    assert_eq!(row.estimate, Some(ESTIMATE));

    store
        .update_card(UpdateCard {
            id,
            title: TEST_TITLE,
            description: "",
            assignee: None,
            deadline: None,
            priority: None,
            labels: Some(""),
            checklist: Some(""),
            estimate: Some(0),
        })
        .await
        .expect("clear");

    let row = store.get(id).await.expect("get").expect("row exists");
    assert_eq!(row.labels, None);
    assert_eq!(row.checklist, None);
    assert_eq!(row.estimate, Some(0));

    store.remove(id).await.expect("remove");
}

#[tokio::test]
async fn card_invalid_labels_json_rejected() {
    let store = Store::connect(Store::default_url()).await.expect("connect");

    let err = store
        .update_card(UpdateCard {
            id: 1,
            title: "x",
            description: "",
            assignee: None,
            deadline: None,
            priority: None,
            labels: Some("{not json"),
            checklist: None,
            estimate: None,
        })
        .await
        .expect_err("bad json rejected");
    assert!(matches!(err, StoreError::BadSpec(_)));

    let err = store
        .add(AddCard {
            project_id: None,
            column_id: TEST_COLUMN,
            title: TEST_TITLE,
            description: "",
            priority: TEST_PRIORITY,
            labels: Some(r#"{"not":"an array"}"#),
            checklist: None,
            estimate: None,
        })
        .await
        .expect_err("non-array rejected");
    assert!(matches!(err, StoreError::BadSpec(_)));
}
