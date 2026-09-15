//! Schedule service tests: routines are the only scheduled entity. Due
//! routines get a next-run slot, disabled/bad-cron ones are dropped, and
//! removed routines are pruned. Card cron is gone — tasks never recur.
//! One serial test: all scenarios share the live routines table.

use std::sync::Arc;

use backend::app::schedule_work::{self, ScheduleHandle};
use backend::app::task::TaskApp;
use backend::port::outbound::{
    MockAgentConfigRepo, MockCardRepo, MockCommentRepo, MockProjectRepo, MockResourceRepo,
    MockSkillRepo, MockWorkspaceRepo,
};

const EVERY_MINUTE: &str = "* * * * *";

fn app(store: Arc<task_rs::Store>) -> Arc<TaskApp> {
    TaskApp::new(
        store,
        Arc::new(MockCardRepo::new()),
        Arc::new(MockCommentRepo::new()),
        Arc::new(MockResourceRepo::new()),
        Arc::new(MockAgentConfigRepo::new()),
        Arc::new(MockSkillRepo::new()),
        Arc::new(MockWorkspaceRepo::new()),
        Arc::new(MockProjectRepo::new()),
    )
    .into()
}

#[tokio::test]
async fn routine_scheduling_scenarios() {
    let url = task_rs::Store::default_url();
    let store = Arc::new(task_rs::Store::connect(&url).await.expect("connect"));
    // the test owns the table: drop leftovers from earlier runs first
    for stale in store.list_routines().await.expect("list") {
        store.remove_routine(stale.id).await.expect("cleanup");
    }
    let app = app(Arc::clone(&store));

    // due routine gets a next-run slot
    let id = store
        .add_routine(&task_rs::RoutineDraft {
            name: "digest".into(),
            cron: EVERY_MINUTE.into(),
            enabled: true,
            ..Default::default()
        })
        .await
        .expect("add");
    let handle = ScheduleHandle::new();
    schedule_work::run_once(&app, None, None, &handle).await;
    let entries = handle.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].card_id, id);
    // first sighting schedules the next minute boundary
    assert!(entries[0].next_run > schedule_work::unix_now());

    // removed routine -> entry pruned on the next pass
    store.remove_routine(id).await.expect("remove");
    schedule_work::run_once(&app, None, None, &handle).await;
    assert!(handle.entries().is_empty());

    // disabled or bad-cron routines never schedule
    let bad = store
        .add_routine(&task_rs::RoutineDraft {
            name: "bad cron".into(),
            cron: "not a cron".into(),
            ..Default::default()
        })
        .await
        .expect("add bad");
    let paused = store
        .add_routine(&task_rs::RoutineDraft {
            name: "paused".into(),
            cron: EVERY_MINUTE.into(),
            enabled: false,
            ..Default::default()
        })
        .await
        .expect("add paused");
    schedule_work::run_once(&app, None, None, &handle).await;
    assert!(handle.entries().is_empty());

    // cleanup
    store.remove_routine(bad).await.expect("cleanup bad");
    store.remove_routine(paused).await.expect("cleanup paused");
}
