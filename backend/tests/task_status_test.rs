//! Task status transitions: canonical TaskStatus mapping, server-side
//! transition validation in CardService, and run-driven status changes.

use std::sync::Arc;

use mockall::predicate::eq;
use task_rs::{COLUMN_DOING, TaskStatus};

use backend::app::card_run;
use backend::app::task::TaskApp;
use backend::domain::{CardMove, CardService};
use backend::port::outbound::MockCardRepo;

const CARD_ID: i64 = 7;

async fn store_for_tests() -> Arc<backend::infra::postgres::Store> {
    Arc::new(
        backend::infra::postgres::Store::connect(&backend::infra::postgres::Store::default_url())
            .await
            .expect("test store"),
    )
}

fn card_row(column_id: &str) -> task_rs::CardRow {
    task_rs::CardRow {
        id: CARD_ID,
        column_id: column_id.into(),
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
        cron: None,
        deadline: None,
        labels: None,
        checklist: None,
        estimate: None,
    }
}

#[test]
fn task_status_maps_columns_and_back() {
    assert_eq!(TaskStatus::parse("todo"), TaskStatus::Todo);
    assert_eq!(TaskStatus::parse(COLUMN_DOING), TaskStatus::InProgress);
    assert_eq!(TaskStatus::InProgress.column(), COLUMN_DOING);
    assert_eq!(TaskStatus::Done.column(), TaskStatus::Done.as_str());
    for s in [
        TaskStatus::Todo,
        TaskStatus::InProgress,
        TaskStatus::Review,
        TaskStatus::Conflict,
        TaskStatus::Done,
        TaskStatus::Failed,
    ] {
        assert_eq!(TaskStatus::parse(s.as_str()), s);
    }
}

#[test]
fn transition_table_rejects_todo_to_review() {
    assert!(task_rs::transition_allowed(
        TaskStatus::Todo,
        TaskStatus::InProgress
    ));
    assert!(!task_rs::transition_allowed(
        TaskStatus::Todo,
        TaskStatus::Review
    ));
    assert!(task_rs::transition_allowed(
        TaskStatus::Review,
        TaskStatus::Done
    ));
    assert!(!task_rs::transition_allowed(
        TaskStatus::Done,
        TaskStatus::Failed
    ));
    assert!(task_rs::transition_allowed(
        TaskStatus::Done,
        TaskStatus::Todo
    ));
    assert!(task_rs::transition_allowed(
        TaskStatus::Conflict,
        TaskStatus::Review
    ));
}

#[tokio::test]
async fn set_status_moves_when_allowed() {
    let mut cards = MockCardRepo::new();
    cards
        .expect_get()
        .with(eq(CARD_ID))
        .times(2)
        .returning(|_| Ok(Some(card_row("todo"))));
    cards
        .expect_move_card()
        .withf(|mv: &CardMove| mv.id == CARD_ID && mv.column_id == COLUMN_DOING && mv.position == 0)
        .returning(|_| Ok(()));
    let svc = CardService::new(Arc::new(cards));
    let row = svc
        .set_status(CARD_ID, TaskStatus::InProgress)
        .await
        .unwrap();
    assert_eq!(row.id, CARD_ID);
}

#[tokio::test]
async fn set_status_rejects_illegal_transition() {
    let mut cards = MockCardRepo::new();
    cards
        .expect_get()
        .with(eq(CARD_ID))
        .returning(|_| Ok(Some(card_row("todo"))));
    let svc = CardService::new(Arc::new(cards));
    let err = svc
        .set_status(CARD_ID, TaskStatus::Review)
        .await
        .unwrap_err();
    assert!(matches!(err, task_rs::StoreError::BadSpec(_)));
}

#[tokio::test]
async fn set_status_noop_when_already_there() {
    let mut cards = MockCardRepo::new();
    cards
        .expect_get()
        .with(eq(CARD_ID))
        .times(2)
        .returning(|_| Ok(Some(card_row("done"))));
    let svc = CardService::new(Arc::new(cards));
    svc.set_status(CARD_ID, TaskStatus::Done).await.unwrap();
}

#[tokio::test]
async fn card_run_reports_missing_agent_and_uses_status_columns() {
    let mut cards = MockCardRepo::new();
    cards.expect_get().returning(|_| Ok(Some(card_row("todo"))));
    cards.expect_set_run_start().returning(|_, _| Ok(()));
    cards
        .expect_move_card()
        .withf(|mv: &CardMove| mv.column_id == COLUMN_DOING)
        .returning(|_| Ok(()));
    cards.expect_set_agent_state().returning(|_, _| Ok(()));
    cards.expect_record_run().returning(|_| Ok(1));
    cards
        .expect_move_card()
        .withf(|mv: &CardMove| mv.column_id == task_rs::COLUMN_FAILED)
        .returning(|_| Ok(()));
    let app = TaskApp::new(
        store_for_tests().await,
        Arc::new(cards),
        Arc::new(backend::port::outbound::MockCommentRepo::new()),
        Arc::new(backend::port::outbound::MockResourceRepo::new()),
        Arc::new(backend::port::outbound::MockAgentConfigRepo::new()),
        Arc::new(backend::port::outbound::MockSkillRepo::new()),
        Arc::new(backend::port::outbound::MockWorkspaceRepo::new()),
        Arc::new(backend::port::outbound::MockProjectRepo::new()),
    );
    let record = card_run::run_card(&app, None, None, CARD_ID, task_rs::TRIGGER_MANUAL, None)
        .await
        .unwrap();
    assert_eq!(record.status, card_run::RunStatus::Failed);
}
