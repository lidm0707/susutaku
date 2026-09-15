//! Agent board flow: a model asked to do scheduled work must create a card,
//! assign an agent to it, and have the ops reach the board in order.

use std::sync::{Arc, Mutex};

use backend::app::ChatUseCase;
use backend::domain::{
    BoardOp, BoardRequest, BoardResult, ChatCmd, GenReply, GitOp, SearchMode, SearchResult,
};
use backend::port::inbound::ChatHandling;
use backend::port::outbound::{BoardOps, Fetcher, Inference, ModelSwitch, Runner, Searcher};
use susutaku_mlx::stats::GenStats;
use susutaku_mlx::tok::TokKind;

const MARK_CARD_CREATED: &str = "card 2 created";
const MARK_AGENT_ASSIGNED: &str = "agent nightly assigned";
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
    fn write_file(&self, _path: &str, _content: &str) -> Result<(), String> {
        Err("no work tree in test".to_string())
    }
    fn read_file(&self, _path: &str) -> Result<String, String> {
        Err("no work tree in test".to_string())
    }
    fn workspace_root(&self) -> std::path::PathBuf {
        std::path::PathBuf::new()
    }
    fn has_git_repo(&self) -> bool {
        false
    }
    fn git(&self, _op: &GitOp) -> Result<String, String> {
        Err("no work tree in test".to_string())
    }
}

/// Round 1: create a card. Round 2 (sees the creation note in the tool
/// results): assign an agent. After that: final answer.
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
        let saw_agent = prompt.contains(MARK_AGENT_ASSIGNED);
        let saw_card = prompt.contains(MARK_CARD_CREATED);
        let saw_offer = prompt.contains(MARK_TOOL_OFFER);
        std::thread::spawn(move || {
            let text = if saw_agent {
                "done".to_string()
            } else if saw_card {
                "TOOL: CARD_AGENT 2 nightly".to_string()
            } else if saw_offer {
                "TOOL: CARD_CREATE 1 trump-card".to_string()
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
        let reply = match &req.op {
            BoardOp::CreateCard { .. } => MARK_CARD_CREATED.to_string(),
            BoardOp::AssignAgent { card_id, agent } => {
                format!("{MARK_AGENT_ASSIGNED} to card {card_id}: {agent}")
            }
            _ => "ok".to_string(),
        };
        self.0.lock().expect("lock").push(req.op);
        Ok(reply)
    }
}

#[tokio::test]
async fn agent_creates_card_then_assigns_agent() {
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
            image: None,
            thread_id: None,
            card_id: None,
        })
        .await
        .expect("chat must complete");

    assert_eq!(outcome.text, "done");
    let ops = board.0.lock().expect("lock");
    assert_eq!(ops.len(), 2, "card + agent must both reach the board");
    assert!(matches!(
        &ops[0],
        BoardOp::CreateCard { project_id: 1, title, description: None } if title == "trump-card"
    ));
    assert!(matches!(
        &ops[1],
        BoardOp::AssignAgent { card_id: 2, agent } if agent == "nightly"
    ));
}
