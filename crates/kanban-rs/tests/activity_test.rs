//! Integration test: activity log record/list, limit, ordering, truncation.

use kanban_rs::{ACTIVITY_LIST_MAX, ACTIVITY_MESSAGE_MAX_CHARS, Store};

const KIND_PREFIX: &str = "test_";
const KIND_A: &str = "test_alpha";
const KIND_B: &str = "test_beta";
const EVENT_COUNT: usize = 5;
const LIMIT: i64 = 3;

async fn cleanup(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM activity WHERE kind LIKE $1")
        .bind(format!("{KIND_PREFIX}%"))
        .execute(pool)
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn activity_record_list_truncate() {
    let store = Store::connect(Store::default_url()).await.expect("connect");
    let pool = sqlx::PgPool::connect(Store::default_url())
        .await
        .expect("pool");
    cleanup(&pool).await;

    for i in 0..EVENT_COUNT {
        let kind = if i % 2 == 0 { KIND_A } else { KIND_B };
        store
            .record_activity(kind, &format!("event {i}"))
            .await
            .expect("record");
    }

    let rows = store.list_activity(LIMIT).await.expect("list");
    assert_eq!(rows.len(), LIMIT as usize);
    assert_eq!(rows[0].message, format!("event {}", EVENT_COUNT - 1));
    assert_eq!(rows[0].kind, KIND_A);
    for pair in rows.windows(2) {
        assert!(pair[0].id > pair[1].id);
        assert!(pair[0].created_at >= pair[1].created_at);
    }

    let big = "x".repeat(ACTIVITY_MESSAGE_MAX_CHARS * 2);
    store.record_activity(KIND_A, &big).await.expect("record");
    let rows = store.list_activity(1).await.expect("list");
    assert_eq!(rows[0].message.len(), ACTIVITY_MESSAGE_MAX_CHARS);

    let clamped = store
        .list_activity(ACTIVITY_LIST_MAX * 10)
        .await
        .expect("list");
    assert!(clamped.len() <= ACTIVITY_LIST_MAX as usize);

    cleanup(&pool).await;
    assert!(store.list_activity(LIMIT).await.expect("list").is_empty());
}
