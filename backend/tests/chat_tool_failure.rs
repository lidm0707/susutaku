//! Tool-failure degradation: a failing search must not abort the chat run —
//! the error is fed back as tool context and the loop continues to a reply.

use std::sync::Arc;

use backend::app::ChatUseCase;
use backend::domain::{BoardResult, ChatCmd, GenReply, SearchMode, SearchResult, TOOL_DENIED};
use backend::port::inbound::ChatHandling;
use backend::port::outbound::{BoardOps, Fetcher, Inference, ModelSwitch, Runner, Searcher};
use susutaku_mlx::stats::GenStats;
use susutaku_mlx::tok::TokKind;

const SEARCH_ERROR: &str = "No useful DuckDuckGo Instant Answer results were found for: donald";

struct FailingSearch;
impl Searcher for FailingSearch {
    fn search(&self, _query: &str) -> Result<Vec<SearchResult>, String> {
        Err(SEARCH_ERROR.to_string())
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

/// First round: ask for a search. Second round (prompt now carries the tool
/// failure note): answer normally.
struct ToolThenReplyEngine;
impl Inference for ToolThenReplyEngine {
    fn submit(
        &self,
        prompt: String,
        _max_tokens: usize,
        _tok: TokKind,
        _think: bool,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<GenReply, String>>, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let wants_tool = prompt.contains("TOOL:");
        let saw_failure = prompt.contains(SEARCH_ERROR);
        std::thread::spawn(move || {
            let text = if wants_tool {
                "TOOL: SEARCH donald".to_string()
            } else {
                assert!(saw_failure, "failure note must reach the model prompt");
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

struct DenyBoard;
#[async_trait::async_trait]
impl BoardOps for DenyBoard {
    async fn exec(&self, _req: backend::domain::BoardRequest) -> BoardResult {
        Err(TOOL_DENIED.to_string())
    }
}

#[tokio::test]
async fn search_failure_does_not_abort_chat() {
    let use_case = ChatUseCase::new(
        Arc::new(FailingSearch),
        Arc::new(NoFetch),
        Arc::new(NoShell),
        Arc::new(ToolThenReplyEngine),
        Arc::new(NoModels),
        None,
        Arc::new(DenyBoard),
        Arc::new(backend::port::outbound::MockAgentConfigRepo::new()),
    );

    let outcome = use_case
        .execute(ChatCmd {
            message: "can you search donald and make card that infor".to_string(),
            mode: SearchMode::Auto,
            max_tokens: 64,
            tokenizer: TokKind::Normal,
            think: false,
            board_token: None,
            agent: None,
        })
        .await
        .expect("chat must survive a failed search tool");

    assert_eq!(outcome.text, "done");
    assert!(outcome.searched);
}
