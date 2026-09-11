//! Integration test: card update (title/description/assignee) + comments.

use kanban_rs::{AddCard, Store, UpdateCard};

const TEST_COLUMN: &str = "todo";
const TEST_TITLE: &str = "detail card";
const TEST_PRIORITY: &str = "normal";
const NEW_TITLE: &str = "renamed card";
const NEW_DESC: &str = "a longer description";
const ASSIGNEE: &str = "alice";
const AUTHOR: &str = "bob";
const COMMENT_BODY: &str = "first comment";

#[tokio::test]
async fn card_update_and_comments_roundtrip() {
    let store = Store::connect(Store::default_url()).await.expect("connect");

    let id = store
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
        .expect("add");

    assert!(
        store
            .get(id)
            .await
            .expect("get")
            .expect("row")
            .assignee
            .is_none()
    );

    store
        .update_card(UpdateCard {
            id,
            title: NEW_TITLE,
            description: NEW_DESC,
            assignee: Some(ASSIGNEE),
            deadline: None,
            priority: None,
            labels: None,
            checklist: None,
            estimate: None,
        })
        .await
        .expect("update");
    let row = store.get(id).await.expect("get").expect("row exists");
    assert_eq!(row.title, NEW_TITLE);
    assert_eq!(row.description, NEW_DESC);
    assert_eq!(row.assignee.as_deref(), Some(ASSIGNEE));

    assert!(store.list_comments(id).await.expect("list").is_empty());
    store
        .add_comment(id, AUTHOR, COMMENT_BODY)
        .await
        .expect("add comment");
    store
        .add_comment(id, AUTHOR, "second")
        .await
        .expect("add comment");
    let comments = store.list_comments(id).await.expect("list");
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].author, AUTHOR);
    assert_eq!(comments[0].body, COMMENT_BODY);
    assert_eq!(comments[0].card_id, id);

    assert!(store.add_comment(999_999_999, AUTHOR, "x").await.is_err());
    assert!(
        store
            .update_card(UpdateCard {
                id: 999_999_999,
                title: "x",
                description: "",
                assignee: None,
                deadline: None,
                priority: None,
                labels: None,
                checklist: None,
                estimate: None,
            })
            .await
            .is_err()
    );

    store.remove(id).await.expect("remove");
    assert!(store.list_comments(id).await.expect("list").is_empty());
}
