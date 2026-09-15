//! Schedule service tests over mocked repos: due cards get scheduled, future
//! ones wait, unscheduled ones are ignored.

use std::sync::Arc;

use task_rs::CardRow;
use mockall::predicate::eq;

use backend::app::task::TaskApp;
use backend::app::schedule_work::{self, ScheduleHandle};
use backend::port::outbound::{
    MockAgentConfigRepo, MockCardRepo, MockCommentRepo, MockProjectRepo, MockResourceRepo,
    MockSkillRepo, MockWorkspaceRepo,
};

const CARD_ID: i64 = 5;
const EVERY_MINUTE: &str = "* * * * *";

fn card_row(cron: Option<&str>) -> CardRow {
    CardRow {
        id: CARD_ID,
        column_id: "todo".into(),
        project_id: None,
        title: "t".into(),
        description: String::new(),
        priority: "normal".into(),
        position: 0,
        agent_name: None,
        agent_state: None,
        run_status: task_rs::RUN_STATUS_IDLE.into(),
        last_agent: None,
        last_run_id: None,
        assignee: None,
        image: None,
        cron: cron.map(str::to_owned),
        deadline: None,
        labels: None,
        checklist: None,
        estimate: None,
    }
}

fn empty_cards() -> MockCardRepo {
    let mut cards = MockCardRepo::new();
    cards.expect_list().returning(|_| Ok(vec![]));
    cards
}

fn app(cards: MockCardRepo) -> Arc<TaskApp> {
    Arc::new(TaskApp::new(
        Arc::new(cards),
        Arc::new(MockCommentRepo::new()),
        Arc::new(MockResourceRepo::new()),
        Arc::new(MockAgentConfigRepo::new()),
        Arc::new(MockSkillRepo::new()),
        Arc::new(MockWorkspaceRepo::new()),
        Arc::new(MockProjectRepo::new()),
    ))
}

#[tokio::test]
async fn unscheduled_cards_are_ignored() {
    let mut cards = MockCardRepo::new();
    cards.expect_list().returning(|_| Ok(vec![card_row(None)]));
    let app = app(cards);
    let handle = ScheduleHandle::new();
    schedule_work::run_once(&app, None, &handle).await;
    assert!(handle.entries().is_empty());
}

#[tokio::test]
async fn scheduled_card_gets_a_next_run() {
    let mut cards = MockCardRepo::new();
    cards
        .expect_list()
        .returning(|_| Ok(vec![card_row(Some(EVERY_MINUTE))]));
    let app = app(cards);
    let handle = ScheduleHandle::new();
    schedule_work::run_once(&app, None, &handle).await;
    let entries = handle.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].card_id, CARD_ID);
    // first sighting schedules the next minute boundary
    assert!(entries[0].next_run > schedule_work::unix_now());
}

#[tokio::test]
async fn bad_cron_is_dropped() {
    let mut cards = MockCardRepo::new();
    cards
        .expect_list()
        .returning(|_| Ok(vec![card_row(Some("not a cron"))]));
    let app = app(cards);
    let handle = ScheduleHandle::new();
    schedule_work::run_once(&app, None, &handle).await;
    assert!(handle.entries().is_empty());
}

#[tokio::test]
async fn removed_cron_is_forgotten() {
    let handle = ScheduleHandle::new();
    let mut with_cron = MockCardRepo::new();
    with_cron
        .expect_list()
        .times(1)
        .returning(|_| Ok(vec![card_row(Some(EVERY_MINUTE))]));
    let app_cron = app(with_cron);
    schedule_work::run_once(&app_cron, None, &handle).await;
    assert_eq!(handle.entries().len(), 1);
    // cron removed -> entry pruned on the next pass
    let app = app(empty_cards());
    schedule_work::run_once(&app, None, &handle).await;
    assert!(handle.entries().is_empty());
}

#[tokio::test]
async fn set_cron_reaches_the_repo() {
    let mut cards = MockCardRepo::new();
    cards
        .expect_set_cron()
        .with(eq(CARD_ID), eq(Some("*/5 * * * *".to_owned())))
        .returning(|_, _| Ok(()));
    let app = app(cards);
    app.cards
        .set_cron(CARD_ID, Some("*/5 * * * *".to_owned()))
        .await
        .expect("set cron");
}
