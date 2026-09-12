//! Chat-memory round trip at the trait boundary: a fake memory verifies the
//! use case recalls before prompting and remembers after replying.

use std::sync::{Arc, Mutex};

use backend::app::ChatUseCase;
use backend::domain::{BoardResult, ChatCmd, GenReply, MemoryHit, SearchMode, SearchResult};
use backend::port::inbound::ChatHandling;
use backend::port::outbound::{
    BoardOps, ChatMemory, Fetcher, Inference, ModelSwitch, Runner, Searcher,
};
use susutaku_mlx::stats::GenStats;
use susutaku_mlx::tok::TokKind;

struct NoSearch;
impl Searcher for NoSearch {
    fn search(&self, _query: &str) -> Result<Vec<SearchResult>, String> {
        Ok(Vec::new())
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

struct FakeEngine;
impl Inference for FakeEngine {
    fn submit(
        &self,
        prompt: String,
        _max_tokens: usize,
        _tok: TokKind,
        _think: bool,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<GenReply, String>>, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let saw_memory = prompt.contains("Earlier relevant conversation:");
        std::thread::spawn(move || {
            let _ = tx.send(Ok(GenReply {
                model: "fake".to_string(),
                text: if saw_memory { "memory ok" } else { "plain" }.to_string(),
                stats: GenStats {
                    prompt_tokens: 0,
                    prompt_secs: 0.0,
                    decode_tokens: 0,
                    decode_secs: 0.0,
                },
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

#[derive(Default)]
struct FakeMemory {
    stored: Mutex<Vec<(String, String)>>,
}

impl ChatMemory for FakeMemory {
    fn remember(&self, role: &str, text: &str) -> Result<(), String> {
        self.stored
            .lock()
            .map_err(|_| "poisoned".to_string())?
            .push((role.to_string(), text.to_string()));
        Ok(())
    }

    fn recall(&self, _query: &str, _k: usize) -> Result<Vec<MemoryHit>, String> {
        Ok(vec![MemoryHit {
            role: "user".to_string(),
            text: "earlier fact".to_string(),
        }])
    }
}

fn use_case(memory: Option<Arc<dyn ChatMemory>>) -> ChatUseCase {
    ChatUseCase::new(
        Arc::new(NoSearch),
        Arc::new(NoFetch),
        Arc::new(NoShell),
        Arc::new(FakeEngine),
        Arc::new(NoModels),
        memory,
        Arc::new(DenyBoard),
        Arc::new(backend::port::outbound::MockAgentConfigRepo::new()),
    )
}

struct DenyBoard;

#[async_trait::async_trait]
impl BoardOps for DenyBoard {
    async fn exec(&self, _req: backend::domain::BoardRequest) -> BoardResult {
        Err("board tools disabled in test".to_string())
    }
}

#[tokio::test]
async fn recalls_before_and_remembers_after() {
    let fake = Arc::new(FakeMemory::default());
    let use_case = use_case(Some(fake.clone() as Arc<dyn ChatMemory>));

    let outcome = use_case
        .execute(ChatCmd {
            message: "hello".to_string(),
            mode: SearchMode::Off,
            max_tokens: 32,
            tokenizer: TokKind::Normal,
            think: false,
            board_token: None,
            agent: None,
        })
        .await
        .expect("chat ok");

    assert_eq!(outcome.text, "memory ok");
    let stored = fake.stored.lock().unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0], ("user".to_string(), "hello".to_string()));
    assert_eq!(
        stored[1],
        ("assistant".to_string(), "memory ok".to_string())
    );
}

#[tokio::test]
async fn works_without_memory() {
    let use_case = use_case(None);
    let outcome = use_case
        .execute(ChatCmd {
            message: "hi".to_string(),
            mode: SearchMode::Off,
            max_tokens: 32,
            tokenizer: TokKind::Normal,
            think: false,
            board_token: None,
            agent: None,
        })
        .await
        .expect("chat ok");
    assert_eq!(outcome.text, "plain");
}
