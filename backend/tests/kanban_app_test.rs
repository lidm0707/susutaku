//! Mock-driven tests: kanban port traits (automock), 1:1 services running in
//! transactions, and app-layer composition (pipeline names joined in Rust).

use std::sync::Arc;

use kanban_rs::{COLUMN_DONE, COLUMN_FAILED, CardRow, PipelineRow, StoreError};
use mockall::predicate::eq;

use backend::app::kanban::KanbanApp;
use backend::app::pipeline_run;
use backend::domain::{CardService, CommentService, NewCard, NewPipeline, PipelineService};
use backend::port::outbound::{
    MockAgentConfigRepo, MockCardRepo, MockCardTx, MockCommentRepo, MockCommentTx,
    MockPipelineRepo, MockPipelineTx, MockProjectRepo, MockResourceRepo, MockSkillRepo,
    MockWorkspaceRepo,
};

const CARD_ID: i64 = 5;
const PIPE_ID: i64 = 7;
const PIPE_NAME: &str = "ingest-render";
const SPEC_OK: &str = r#"{"nodes":[{"id":"a","stage":"ingest"},{"id":"b","stage":"render"}],"links":[{"from":"a","to":"b"}]}"#;
const SPEC_AGENT: &str = r#"{"nodes":[{"id":"a","stage":"ingest"},{"id":"bot","stage":"agent","params":{"agent":"qwen"}}],"links":[{"from":"a","to":"bot"}]}"#;
const SPEC_FAIL: &str = r#"{"nodes":[{"id":"a","stage":"ingest"},{"id":"f","stage":"parse"}],"links":[{"from":"a","to":"f"}]}"#;
const SPEC_REF_IMAGE: &str = r#"{"nodes":[{"id":"a","stage":"ingest"},{"id":"i","stage":"ref_image","params":{"path":"REPLACED"}}],"links":[{"from":"a","to":"i"}]}"#;

fn card_row(id: i64, pipeline_id: Option<i64>) -> CardRow {
    CardRow {
        id,
        column_id: "todo".into(),
        project_id: None,
        title: "t".into(),
        description: String::new(),
        priority: "normal".into(),
        position: 0,
        agent_name: None,
        agent_state: None,
        assignee: None,
        pipeline_id,
        cron: None,
        deadline: None,
        labels: None,
        checklist: None,
        estimate: None,
    }
}

fn pipeline_row(id: i64, name: &str) -> PipelineRow {
    PipelineRow {
        id,
        name: name.into(),
        spec: "{}".into(),
    }
}

#[tokio::test]
async fn card_views_join_pipeline_names_in_app_layer() {
    let mut cards = MockCardRepo::new();
    let mut pipelines = MockPipelineRepo::new();
    cards
        .expect_list()
        .with(eq(Some(1)))
        .returning(|_| Ok(vec![card_row(CARD_ID, Some(PIPE_ID)), card_row(6, None)]));
    pipelines
        .expect_list()
        .returning(|| Ok(vec![pipeline_row(PIPE_ID, PIPE_NAME)]));

    let app = KanbanApp::new(
        Arc::new(cards),
        Arc::new(MockCommentRepo::new()),
        Arc::new(pipelines),
        Arc::new(MockResourceRepo::new()),
        Arc::new(MockAgentConfigRepo::new()),
        Arc::new(MockSkillRepo::new()),
        Arc::new(MockWorkspaceRepo::new()),
        Arc::new(MockProjectRepo::new()),
    );

    let views = app.card_views(Some(1)).await.expect("views");
    assert_eq!(views.len(), 2);
    assert_eq!(views[0].pipeline_name.as_deref(), Some(PIPE_NAME));
    assert_eq!(views[1].pipeline_name, None);
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
                Ok(kanban_rs::CommentRow {
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

#[tokio::test]
async fn pipeline_service_create_reads_back_inside_tx() {
    let mut pipelines = MockPipelineRepo::new();
    pipelines.expect_tx().returning(|| {
        let mut tx = MockPipelineTx::new();
        tx.expect_create()
            .withf(|p: &NewPipeline| p.name == PIPE_NAME)
            .returning(|_| Ok(PIPE_ID));
        tx.expect_get()
            .with(eq(PIPE_ID))
            .returning(|id| Ok(Some(pipeline_row(id, PIPE_NAME))));
        tx.expect_commit().returning(|| Ok(()));
        Ok(Box::new(tx))
    });

    let svc = PipelineService::new(Arc::new(pipelines));
    let row = svc
        .create(NewPipeline {
            name: PIPE_NAME.into(),
            spec: "{}".into(),
        })
        .await
        .expect("created");
    assert_eq!(row.id, PIPE_ID);
}

/// A run from `todo` moves the card: todo → doing at start, → `target` at end.
fn expect_run_moves_todo_to(cards: &mut MockCardRepo, target: &str) {
    cards
        .expect_move_card()
        .withf(|mv: &backend::domain::CardMove| mv.column_id == kanban_rs::COLUMN_DOING)
        .returning(|_| Ok(()));
    let target = target.to_string();
    cards
        .expect_move_card()
        .withf(move |mv: &backend::domain::CardMove| mv.column_id == target)
        .returning(|_| Ok(()));
}

fn runner_app(cards: MockCardRepo, pipelines: MockPipelineRepo) -> KanbanApp {
    KanbanApp::new(
        Arc::new(cards),
        Arc::new(MockCommentRepo::new()),
        Arc::new(pipelines),
        Arc::new(MockResourceRepo::new()),
        Arc::new(MockAgentConfigRepo::new()),
        Arc::new(MockSkillRepo::new()),
        Arc::new(MockWorkspaceRepo::new()),
        Arc::new(MockProjectRepo::new()),
    )
}

#[tokio::test]
async fn run_card_pipeline_records_ok_and_persists_state() {
    let mut cards = MockCardRepo::new();
    let mut pipelines = MockPipelineRepo::new();
    cards
        .expect_get()
        .with(eq(CARD_ID))
        .returning(|id| Ok(Some(card_row(id, Some(PIPE_ID)))));
    expect_run_moves_todo_to(&mut cards, COLUMN_DONE);
    cards
        .expect_set_agent()
        .withf(|_, a: &kanban_rs::AgentState| {
            a.name == "pipeline-runner"
                && a.state["run"]["status"] == "ok"
                && a.state["run"]["stages"]
                    .as_array()
                    .is_some_and(|s| s.len() == 2)
        })
        .returning(|_, _| Ok(()));
    pipelines.expect_list().returning(move || {
        let mut row = pipeline_row(PIPE_ID, PIPE_NAME);
        row.spec = SPEC_OK.into();
        Ok(vec![row])
    });

    let record = pipeline_run::run_card_pipeline(&runner_app(cards, pipelines), CARD_ID)
        .await
        .expect("ran");
    assert_eq!(record.status, pipeline_run::StageStatus::Ok);
    assert_eq!(record.pipeline_name, PIPE_NAME);
}

#[tokio::test]
async fn run_card_pipeline_agent_node_sets_agent_name() {
    let mut cards = MockCardRepo::new();
    let mut pipelines = MockPipelineRepo::new();
    cards
        .expect_get()
        .with(eq(CARD_ID))
        .returning(|id| Ok(Some(card_row(id, Some(PIPE_ID)))));
    expect_run_moves_todo_to(&mut cards, COLUMN_DONE);
    cards
        .expect_set_agent()
        .withf(|_, a: &kanban_rs::AgentState| a.name == "qwen")
        .returning(|_, _| Ok(()));
    pipelines.expect_list().returning(move || {
        let mut row = pipeline_row(PIPE_ID, PIPE_NAME);
        row.spec = SPEC_AGENT.into();
        Ok(vec![row])
    });

    let record = pipeline_run::run_card_pipeline(&runner_app(cards, pipelines), CARD_ID)
        .await
        .expect("ran");
    assert_eq!(record.status, pipeline_run::StageStatus::Ok);
}

#[tokio::test]
async fn run_card_pipeline_unwired_stage_fails_run_with_note() {
    let mut cards = MockCardRepo::new();
    let mut pipelines = MockPipelineRepo::new();
    cards
        .expect_get()
        .with(eq(CARD_ID))
        .returning(|id| Ok(Some(card_row(id, Some(PIPE_ID)))));
    expect_run_moves_todo_to(&mut cards, COLUMN_FAILED);
    cards
        .expect_set_agent()
        .withf(|_, a: &kanban_rs::AgentState| a.state["run"]["status"] == "failed")
        .returning(|_, _| Ok(()));
    pipelines.expect_list().returning(move || {
        let mut row = pipeline_row(PIPE_ID, PIPE_NAME);
        row.spec = SPEC_FAIL.into();
        Ok(vec![row])
    });

    let record = pipeline_run::run_card_pipeline(&runner_app(cards, pipelines), CARD_ID)
        .await
        .expect("run record persisted");
    assert_eq!(record.status, pipeline_run::StageStatus::Failed);
    assert!(
        record
            .stages
            .last()
            .expect("stage")
            .note
            .contains("not wired")
    );
}

#[tokio::test]
async fn run_card_pipeline_ref_image_loads_file_into_payload() {
    let dir = std::env::temp_dir().join(format!(
        "susutaku-ref-image-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let img_path = dir.join("img.bin");
    std::fs::write(&img_path, [0u8, 1, 2, 3]).expect("write image");

    let mut cards = MockCardRepo::new();
    let mut pipelines = MockPipelineRepo::new();
    cards
        .expect_get()
        .with(eq(CARD_ID))
        .returning(|id| Ok(Some(card_row(id, Some(PIPE_ID)))));
    expect_run_moves_todo_to(&mut cards, COLUMN_DONE);
    cards
        .expect_set_agent()
        .withf(|_, a: &kanban_rs::AgentState| {
            a.state["run"]["stages"][1]["status"] == "ok"
                && a.state["run"]["stages"][1]["note"]
                    .as_str()
                    .is_some_and(|n| n.contains("loaded image"))
        })
        .returning(|_, _| Ok(()));
    let spec: &'static str = Box::leak(
        SPEC_REF_IMAGE
            .replace("REPLACED", &img_path.to_string_lossy())
            .into_boxed_str(),
    );
    pipelines.expect_list().returning(move || {
        let mut row = pipeline_row(PIPE_ID, PIPE_NAME);
        row.spec = spec.into();
        Ok(vec![row])
    });

    let record = pipeline_run::run_card_pipeline(&runner_app(cards, pipelines), CARD_ID)
        .await
        .expect("ran");
    assert_eq!(record.status, pipeline_run::StageStatus::Ok);

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn run_card_pipeline_without_pipeline_is_no_such_pipeline() {
    let mut cards = MockCardRepo::new();
    cards
        .expect_get()
        .with(eq(CARD_ID))
        .returning(|id| Ok(Some(card_row(id, None))));

    let err = pipeline_run::run_card_pipeline(&runner_app(cards, MockPipelineRepo::new()), CARD_ID)
        .await
        .expect_err("no pipeline");
    assert!(matches!(err, StoreError::NoSuchPipeline));
}
