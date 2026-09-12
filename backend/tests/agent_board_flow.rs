//! Agent board flow: a model asking for scheduled work must be able to create
//! a pipeline WITH a valid spec (not just an empty shell), a card, and have
//! the ops reach the board in order.

use std::sync::{Arc, Mutex};

use backend::app::ChatUseCase;
use backend::domain::{
    BoardOp, BoardRequest, BoardResult, ChatCmd, GenReply, SearchMode, SearchResult,
};
use backend::port::inbound::ChatHandling;
use backend::port::outbound::{BoardOps, Fetcher, Inference, ModelSwitch, Runner, Searcher};
use susutaku_mlx::stats::GenStats;
use susutaku_mlx::tok::TokKind;

const SPEC_SEARCH: &str =
    r#"{"nodes":[{"id":"a","stage":"search","params":{"query":"donald trump"}}],"links":[]}"#;
const MARK_PIPELINE_CREATED: &str = "pipeline 1 created";
const MARK_CARD_CREATED: &str = "card 2 created";
const MARK_TOOL_OFFER: &str = "TOOL:";

struct NoSearch;
impl Searcher for NoSearch {
    fn search(&self, _q: &str) -> Result<Vec<SearchResult>, String> {
        Err("no search in test".to_string())
    }
}

struct NoFetch;
impl Fetcher for NoFetch {
    fn fetch(&self, _url: &str) -> Result<String, String> {
        Err("no fetch in test".to_string())
    }
}

struct NoShell;
impl Runner for NoShell {
    fn run(&self, _cmd: &str) -> Result<String, String> {
        Err("no shell in test".to_string())
    }
}

/// Round 1: create a pipeline with a spec. Round 2 (sees the creation note in
/// the tool results): create a card. After that: final answer.
struct BoardFlowEngine;
impl Inference for BoardFlowEngine {
    fn submit(
        &self,
        prompt: String,
        _max_tokens: usize,
        _tok: TokKind,
        _think: bool,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<GenReply, String>>, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let saw_pipeline = prompt.contains(MARK_PIPELINE_CREATED);
        let saw_card = prompt.contains(MARK_CARD_CREATED);
        let saw_offer = prompt.contains(MARK_TOOL_OFFER);
        std::thread::spawn(move || {
            let text = if saw_card {
                "done".to_string()
            } else if saw_pipeline {
                "TOOL: CARD_CREATE 1 trump-card".to_string()
            } else if saw_offer {
                format!(r"TOOL: PIPELINE_CREATE nightly {SPEC_SEARCH}")
            } else {
                "done".to_string()
            };
            let _ = tx.send(Ok(GenReply {
                model: "fake".to_string(),
                text,
                stats: GenStats::default(),
            }));
        });
        Ok(rx)
    }
}

struct NoModels;
impl ModelSwitch for NoModels {
    fn select(&self, _name: &str) -> Result<(), String> {
        Ok(())
    }
    fn selected(&self) -> Option<String> {
        None
    }
}

struct RecordingBoard(Mutex<Vec<BoardOp>>);

#[async_trait::async_trait]
impl BoardOps for RecordingBoard {
    async fn exec(&self, req: BoardRequest) -> BoardResult {
        let reply = match req.op {
            BoardOp::CreatePipeline { .. } => MARK_PIPELINE_CREATED.to_string() + ": nightly",
            BoardOp::CreateCard { .. } => "card 2 created".to_string(),
            _ => "ok".to_string(),
        };
        self.0.lock().expect("lock").push(req.op);
        Ok(reply)
    }
}

#[tokio::test]
async fn agent_creates_pipeline_with_spec_then_card() {
    let board = Arc::new(RecordingBoard(Mutex::new(Vec::new())));
    let use_case = ChatUseCase::new(
        Arc::new(NoSearch),
        Arc::new(NoFetch),
        Arc::new(NoShell),
        Arc::new(BoardFlowEngine),
        Arc::new(NoModels),
        None,
        Arc::clone(&board) as Arc<dyn BoardOps>,
        Arc::new(backend::port::outbound::MockAgentConfigRepo::new()),
    );

    let outcome = use_case
        .execute(ChatCmd {
            message: "search donald trump and create that card".to_string(),
            mode: SearchMode::Auto,
            max_tokens: 64,
            tokenizer: TokKind::Normal,
            think: false,
            board_token: Some("tok".to_string()),
            agent: None,
        })
        .await
        .expect("chat must complete");

    assert_eq!(outcome.text, "done");
    let ops = board.0.lock().expect("lock");
    assert_eq!(ops.len(), 2, "pipeline + card must both reach the board");
    assert!(matches!(
        &ops[0],
        BoardOp::CreatePipeline { name, spec } if name == "nightly" && spec.as_deref() == Some(SPEC_SEARCH)
    ));
    assert!(matches!(
        &ops[1],
        BoardOp::CreateCard { project_id: 1, title } if title == "trump-card"
    ));
}
