//! Mock-driven tests: task port traits (automock), 1:1 services running in
//! transactions, and the card runner (runs the card's assigned agent).

use std::sync::Arc;

use mockall::predicate::eq;
use task_rs::{COLUMN_DONE, COLUMN_FAILED, CardRow, RunRecordNew, StoreError};

use backend::app::card_run;
use backend::app::task::TaskApp;
use backend::domain::GenReply;
use backend::domain::{CardService, CommentService, NewCard};
use backend::port::outbound::{
    MockAgentConfigRepo, MockCardRepo, MockCardTx, MockCommentRepo, MockCommentTx, MockProjectRepo,
    MockResourceRepo, MockSkillRepo, MockWorkspaceRepo,
};
use susutaku_mlx::stats::GenStats;
use susutaku_mlx::tok::TokKind;

const CARD_ID: i64 = 5;
const AGENT_NAME: &str = "qwen";

fn card_row(id: i64, agent_name: Option<&str>) -> CardRow {
    CardRow {
        id,
        column_id: "todo".into(),
        project_id: None,
        title: "t".into(),
        description: String::new(),
        priority: "normal".into(),
        position: 0,
        agent_name: agent_name.map(str::to_owned),
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

#[tokio::test]
async fn card_service_create_runs_write_and_read_back_in_one_tx() {
    let mut cards = MockCardRepo::new();
    cards.expect_tx().returning(|| {
        let mut tx = MockCardTx::new();
        tx.expect_add()
            .withf(|c: &NewCard| c.title == "hello")
            .returning(|_| Ok(CARD_ID));
        tx.expect_get()
            .with(eq(CARD_ID))
            .returning(|id| Ok(Some(card_row(id, None))));
        tx.expect_commit().returning(|| Ok(()));
        Ok(Box::new(tx))
    });

    let svc = CardService::new(Arc::new(cards));
    let row = svc
        .create(NewCard {
            project_id: None,
            column_id: "todo".into(),
            title: "hello".into(),
            description: String::new(),
            priority: "normal".into(),
            labels: None,
            checklist: None,
            estimate: None,
        })
        .await
        .expect("created");
    assert_eq!(row.id, CARD_ID);
}

#[tokio::test]
async fn comment_service_add_checks_card_exists_before_insert() {
    let mut comments = MockCommentRepo::new();
    comments.expect_tx().returning(|| {
        let mut tx = MockCommentTx::new();
        tx.expect_card_exists()
            .with(eq(CARD_ID))
            .returning(|_| Ok(true));
        tx.expect_add()
            .with(eq(CARD_ID), eq("alice"), eq("hi"))
            .returning(|card_id, author, body| {
                Ok(task_rs::CommentRow {
                    id: 1,
                    card_id,
                    author: author.into(),
                    body: body.into(),
                    created_at: chrono::Utc::now(),
                })
            });
        tx.expect_commit().returning(|| Ok(()));
        Ok(Box::new(tx))
    });

    let svc = CommentService::new(Arc::new(comments));
    let row = svc
        .add(CARD_ID, "alice".into(), "hi".into())
        .await
        .expect("added");
    assert_eq!(row.body, "hi");
}

#[tokio::test]
async fn comment_service_add_missing_card_rolls_back() {
    let mut comments = MockCommentRepo::new();
    comments.expect_tx().returning(|| {
        let mut tx = MockCommentTx::new();
        tx.expect_card_exists().returning(|_| Ok(false));
        tx.expect_rollback().returning(|| Ok(()));
        Ok(Box::new(tx))
    });

    let svc = CommentService::new(Arc::new(comments));
    let err = svc.add(CARD_ID, "alice".into(), "hi".into()).await;
    assert!(matches!(err, Err(StoreError::NoSuchCard)));
}

/// A run from `todo` moves the card: todo → doing at start, → `target` at end.
fn expect_run_moves_todo_to(cards: &mut MockCardRepo, target: &str) {
    cards.expect_set_run_start().returning(|_, _| Ok(()));
    cards
        .expect_move_card()
        .withf(|mv: &backend::domain::CardMove| mv.column_id == task_rs::COLUMN_DOING)
        .returning(|_| Ok(()));
    let target = target.to_string();
    cards
        .expect_move_card()
        .withf(move |mv: &backend::domain::CardMove| mv.column_id == target)
        .returning(|_| Ok(()));
}

async fn runner_app(cards: MockCardRepo) -> TaskApp {
    let mut agents = MockAgentConfigRepo::new();
    agents.expect_by_name().returning(|_| {
        Ok(Some(task_rs::AgentConfigRow {
            id: 1,
            name: AGENT_NAME.into(),
            model: String::new(),
            persona: String::new(),
            prompt: String::new(),
            output: String::new(),
            allowed_tools: Vec::new(),
            receive_images: false,
            thinking: "off".into(),
        }))
    });
    runner_app_with(test_store().await, cards, agents).await
}

async fn test_store() -> Arc<task_rs::Store> {
    Arc::new(
        task_rs::Store::connect(&task_rs::Store::default_url())
            .await
            .expect("test store"),
    )
}

async fn runner_app_with(
    store: Arc<task_rs::Store>,
    cards: MockCardRepo,
    agents: MockAgentConfigRepo,
) -> TaskApp {
    TaskApp::new(
        store,
        Arc::new(cards),
        Arc::new(MockCommentRepo::new()),
        Arc::new(MockResourceRepo::new()),
        Arc::new(agents),
        Arc::new(MockSkillRepo::new()),
        Arc::new(MockWorkspaceRepo::new()),
        Arc::new(MockProjectRepo::new()),
    )
}

/// Engine that answers every submit with a fixed reply.
struct FixedEngine;
impl backend::port::outbound::Inference for FixedEngine {
    fn submit(
        &self,
        prompt: String,
        _max_tokens: usize,
        _tok: TokKind,
        _think: bool,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<GenReply, String>>, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(Ok(GenReply {
                model: "fake".to_string(),
                text: format!("reply-to: {prompt}"),
                stats: GenStats::default(),
            }));
        });
        Ok(rx)
    }
}

#[tokio::test]
async fn run_card_records_ok_and_persists_state() {
    let mut cards = MockCardRepo::new();
    cards
        .expect_get()
        .with(eq(CARD_ID))
        .returning(|id| Ok(Some(card_row(id, Some(AGENT_NAME)))));
    expect_run_moves_todo_to(&mut cards, COLUMN_DONE);
    cards
        .expect_set_agent_state()
        .withf(|_, s: &str| {
            serde_json::from_str::<serde_json::Value>(s).is_ok_and(|v| {
                v["run"]["status"] == "ok"
                    && v["run"]["output"]
                        .as_str()
                        .is_some_and(|o| o.contains("reply-to"))
            })
        })
        .returning(|_, _| Ok(()));
    cards
        .expect_record_run()
        .withf(|r: &RunRecordNew| {
            r.trigger == task_rs::TRIGGER_MANUAL && r.agent == AGENT_NAME && r.ok
        })
        .returning(|_| Ok(1));

    let record = card_run::run_card(
        &runner_app(cards).await,
        Some(Arc::new(FixedEngine)),
        None,
        CARD_ID,
        task_rs::TRIGGER_MANUAL,
        None
    )
    .await
    .expect("ran");
    assert_eq!(record.status, card_run::RunStatus::Ok);
    assert_eq!(record.agent, AGENT_NAME);
}

#[tokio::test]
async fn run_card_without_engine_fails_run_with_note() {
    let mut cards = MockCardRepo::new();
    cards
        .expect_get()
        .with(eq(CARD_ID))
        .returning(|id| Ok(Some(card_row(id, Some(AGENT_NAME)))));
    expect_run_moves_todo_to(&mut cards, COLUMN_FAILED);
    cards
        .expect_set_agent_state()
        .withf(|_, s: &str| {
            serde_json::from_str::<serde_json::Value>(s)
                .is_ok_and(|v| v["run"]["status"] == "failed")
        })
        .returning(|_, _| Ok(()));
    cards
        .expect_record_run()
        .withf(|r: &RunRecordNew| !r.ok && r.agent == AGENT_NAME)
        .returning(|_| Ok(1));

    let record = card_run::run_card(
        &runner_app(cards).await,
        None,
        None,
        CARD_ID,
        task_rs::TRIGGER_MANUAL,
        None
    )
    .await
    .expect("run record persisted");
    assert_eq!(record.status, card_run::RunStatus::Failed);
    assert!(
        record
            .output
            .as_deref()
            .is_some_and(|n| n.contains("no inference engine"))
    );
}

#[tokio::test]
async fn run_card_without_agent_persists_failed_run() {
    let mut cards = MockCardRepo::new();
    cards
        .expect_get()
        .with(eq(CARD_ID))
        .returning(|id| Ok(Some(card_row(id, None))));
    // No agent: the run cannot start, but a failed run is still persisted and
    // the card moves to the failed column.
    expect_run_moves_todo_to(&mut cards, COLUMN_FAILED);
    cards
        .expect_set_agent_state()
        .withf(|_, s: &str| {
            serde_json::from_str::<serde_json::Value>(s).is_ok_and(|v| {
                v["run"]["status"] == "failed"
                    && v["run"]["output"]
                        .as_str()
                        .is_some_and(|n| n.contains("no agent"))
            })
        })
        .returning(|_, _| Ok(()));
    cards
        .expect_record_run()
        .withf(|r: &RunRecordNew| !r.ok && r.trigger == task_rs::TRIGGER_MANUAL)
        .returning(|_| Ok(1));

    let record = card_run::run_card(
        &runner_app(cards).await,
        None,
        None,
        CARD_ID,
        task_rs::TRIGGER_MANUAL,
        None
    )
    .await
    .expect("failed run persisted");
    assert_eq!(record.status, card_run::RunStatus::Failed);
}

/// Router that serves a distinct engine for the configured cloud model.
struct RoutedEngine(&'static str);
impl backend::port::outbound::ModelEngines for RoutedEngine {
    fn engine_for(
        &self,
        model: &str,
    ) -> Option<std::sync::Arc<dyn backend::port::outbound::Inference>> {
        (model == self.0).then(|| std::sync::Arc::new(FixedEngine) as _)
    }
}

/// Regression: a manual run must go through the agent's configured model —
/// the router takes precedence over the shared engine when it knows the model.
#[tokio::test]
async fn run_card_routes_through_agent_model() {
    const CLOUD_MODEL: &str = "glm-test";
    let mut cards = MockCardRepo::new();
    cards
        .expect_get()
        .with(eq(CARD_ID))
        .returning(|id| Ok(Some(card_row(id, Some(AGENT_NAME)))));
    expect_run_moves_todo_to(&mut cards, COLUMN_DONE);
    cards.expect_set_agent_state().returning(|_, _| Ok(()));
    cards
        .expect_record_run()
        .withf(|r: &RunRecordNew| r.ok && r.agent == AGENT_NAME)
        .returning(|_| Ok(1));

    let mut agents = MockAgentConfigRepo::new();
    agents.expect_by_name().returning(move |_| {
        Ok(Some(task_rs::AgentConfigRow {
            id: 1,
            name: AGENT_NAME.into(),
            model: CLOUD_MODEL.into(),
            persona: String::new(),
            prompt: String::new(),
            output: String::new(),
            allowed_tools: Vec::new(),
            receive_images: false,
            thinking: "off".into(),
        }))
    });

    struct NoEngine;
    impl backend::port::outbound::Inference for NoEngine {
        fn submit(
            &self,
            _: String,
            _: usize,
            _: TokKind,
            _: bool,
        ) -> Result<tokio::sync::oneshot::Receiver<Result<GenReply, String>>, String> {
            Err("shared engine must not be used".to_string())
        }
    }

    let record = card_run::run_card(
        &runner_app_with(test_store().await, cards, agents).await,
        Some(Arc::new(NoEngine)),
        Some(&RoutedEngine(CLOUD_MODEL)),
        CARD_ID,
        task_rs::TRIGGER_MANUAL,
        None
    )
    .await
    .expect("ran");
    assert_eq!(record.status, card_run::RunStatus::Ok);
}
