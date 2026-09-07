//! Mock-driven tests: kanban port traits (automock), 1:1 services running in
//! transactions, and app-layer composition (pipeline names joined in Rust).

use std::sync::Arc;

use kanban_rs::{CardRow, PipelineRow, StoreError};
use mockall::predicate::eq;

use backend::app::kanban::{CardService, CommentService, KanbanApp, PipelineService};
use backend::port::outbound::{
    MockAgentConfigRepo, MockCardRepo, MockCardTx, MockCommentRepo, MockCommentTx,
    MockPipelineRepo, MockPipelineTx, MockProjectRepo, MockWorkspaceRepo, NewCard, NewPipeline,
};

const CARD_ID: i64 = 5;
const PIPE_ID: i64 = 7;
const PIPE_NAME: &str = "ingest-render";

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
        Arc::new(MockAgentConfigRepo::new()),
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
        tx.expect_get().with(eq(CARD_ID)).returning(|id| Ok(Some(card_row(id, None))));
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
        tx.expect_card_exists().with(eq(CARD_ID)).returning(|_| Ok(true));
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
    let row = svc.add(CARD_ID, "alice".into(), "hi".into()).await.expect("added");
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
