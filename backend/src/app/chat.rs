//! Compound service: the chat use case. Depends on ports (traits) and the
//! domain, never on concrete infrastructure.

use std::sync::Arc;

use crate::domain::{
    Prompt, SearchMode, SearchResult, TOOL_RESULT_HEADER, TOOL_ROUNDS_MAX, ToolCall,
};
use crate::port::inbound::{ChatCmd, ChatHandling, ChatOutcome};
use crate::port::outbound::{
    ChatMemory, Fetcher, GenReply, Inference, ModelSwitch, Runner, Searcher,
};
use susutaku_mlx::tok::TokKind;

const MEMORY_RECALL_TOP_K: usize = 5;
const MEMORY_CONTEXT_HEADER: &str = "Earlier relevant conversation:\n";

pub struct ChatUseCase {
    searcher: Arc<dyn Searcher>,
    fetcher: Arc<dyn Fetcher>,
    runner: Arc<dyn Runner>,
    engine: Arc<dyn Inference>,
    models: Arc<dyn ModelSwitch>,
    memory: Option<Arc<dyn ChatMemory>>,
}

impl ChatUseCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        searcher: Arc<dyn Searcher>,
        fetcher: Arc<dyn Fetcher>,
        runner: Arc<dyn Runner>,
        engine: Arc<dyn Inference>,
        models: Arc<dyn ModelSwitch>,
        memory: Option<Arc<dyn ChatMemory>>,
    ) -> Self {
        Self {
            searcher,
            fetcher,
            runner,
            engine,
            models,
            memory,
        }
    }

    fn next_call(&self, reply: &GenReply, allow_tools: bool, rounds: usize) -> Option<ToolCall> {
        if !allow_tools || rounds >= TOOL_ROUNDS_MAX {
            return None;
        }
        ToolCall::parse(&reply.text)
    }

    async fn infer(
        &self,
        prompt: String,
        max_tokens: usize,
        tok: TokKind,
        think: bool,
    ) -> Result<GenReply, String> {
        let rx = self.engine.submit(prompt, max_tokens, tok, think)?;
        rx.await
            .map_err(|_| "inference dropped the job".to_string())?
    }

    async fn search_blocking(&self, query: &str) -> Result<Vec<SearchResult>, String> {
        let searcher = Arc::clone(&self.searcher);
        let owned = query.to_string();
        tokio::task::spawn_blocking(move || searcher.search(&owned))
            .await
            .map_err(|_| "search task panicked".to_string())?
    }

    async fn fetch_blocking(&self, url: &str) -> Result<String, String> {
        let fetcher = Arc::clone(&self.fetcher);
        let owned = url.to_string();
        tokio::task::spawn_blocking(move || fetcher.fetch(&owned))
            .await
            .map_err(|_| "fetch task panicked".to_string())?
    }

    async fn shell_blocking(&self, cmd: &str) -> Result<String, String> {
        let runner = Arc::clone(&self.runner);
        let owned = cmd.to_string();
        tokio::task::spawn_blocking(move || runner.run(&owned))
            .await
            .map_err(|_| "shell task panicked".to_string())?
    }

    /// Recalls similar past exchanges; memory problems degrade to empty.
    async fn recall_blocking(&self, query: &str) -> Vec<String> {
        let Some(memory) = self.memory.as_ref().map(Arc::clone) else {
            return Vec::new();
        };
        let owned = query.to_string();
        let recalled =
            tokio::task::spawn_blocking(move || memory.recall(&owned, MEMORY_RECALL_TOP_K))
                .await
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_default();
        recalled
            .iter()
            .map(|hit| format!("{}: {}", hit.role, hit.text))
            .collect()
    }

    async fn remember_blocking(&self, role: &str, text: &str) {
        let Some(memory) = self.memory.as_ref().map(Arc::clone) else {
            return;
        };
        let (role, text) = (role.to_string(), text.to_string());
        let _ = tokio::task::spawn_blocking(move || memory.remember(&role, &text)).await;
    }
}

impl ChatHandling for ChatUseCase {
    /// Zed-style agentic loop: the model may call tools before answering.
    async fn execute(&self, cmd: ChatCmd) -> Result<ChatOutcome, String> {
        let allow_tools = cmd.mode == SearchMode::Auto;

        let mut context = String::new();
        let memories = self.recall_blocking(&cmd.message).await;
        if !memories.is_empty() {
            context.push_str(MEMORY_CONTEXT_HEADER);
            for line in memories {
                context.push_str(&line);
                context.push('\n');
            }
        }
        let mut searched = false;
        if cmd.mode == SearchMode::Force {
            let results = self.search_blocking(&cmd.message).await?;
            searched = true;
            context.push_str(&Prompt::format_results(&cmd.message, &results));
        }

        let mut prompt = Prompt::build(&cmd.message, &context, allow_tools);
        let mut reply = self
            .infer(prompt, cmd.max_tokens, cmd.tokenizer, cmd.think)
            .await?;

        let mut rounds = 0usize;
        while let Some(call) = self.next_call(&reply, allow_tools, rounds) {
            rounds += 1;
            match call {
                ToolCall::Search(query) => {
                    searched = true;
                    let results = self.search_blocking(&query).await?;
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&Prompt::format_results(&cmd.message, &results));
                }
                ToolCall::Fetch(url) => {
                    let page = self.fetch_blocking(&url).await?;
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&page);
                }
                ToolCall::Shell(cmd) => {
                    let out = self.shell_blocking(&cmd).await?;
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&out);
                }
            }
            prompt = Prompt::build(&cmd.message, &context, allow_tools);
            reply = self
                .infer(prompt, cmd.max_tokens, cmd.tokenizer, cmd.think)
                .await?;
        }

        // Tool budget exhausted while the model still wants a tool: force a
        // final answer on the gathered context, without the tool offer.
        if allow_tools && rounds > 0 && ToolCall::parse(&reply.text).is_some() {
            reply = self
                .infer(
                    Prompt::build(&cmd.message, &context, false),
                    cmd.max_tokens,
                    cmd.tokenizer,
                    cmd.think,
                )
                .await?;
        }

        self.remember_blocking("user", &cmd.message).await;
        self.remember_blocking("assistant", &reply.text).await;

        Ok(ChatOutcome {
            model: Some(reply.model),
            text: reply.text,
            searched,
            stats: reply.stats,
        })
    }
}

impl ModelSwitch for ChatUseCase {
    fn select(&self, name: &str) -> Result<(), String> {
        self.models.select(name)
    }

    fn selected(&self) -> Option<String> {
        self.models.selected()
    }
}
