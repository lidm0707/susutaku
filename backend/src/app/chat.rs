//! Compound service: the chat use case. Depends on ports (traits) and the
//! domain, never on concrete infrastructure.

use std::sync::Arc;

use crate::domain::{
    Prompt, SearchMode, SearchResult, TOOL_RESULT_HEADER, TOOL_ROUNDS_MAX, ToolCall,
};
use crate::port::inbound::{ChatCmd, ChatHandling, ChatOutcome};
use crate::port::outbound::{Fetcher, GenReply, Inference, ModelSwitch, Runner, Searcher};
use susutaku_mlx::tok::TokKind;

pub struct ChatUseCase {
    searcher: Arc<dyn Searcher>,
    fetcher: Arc<dyn Fetcher>,
    runner: Arc<dyn Runner>,
    engine: Arc<dyn Inference>,
    models: Arc<dyn ModelSwitch>,
}

impl ChatUseCase {
    pub fn new(
        searcher: Arc<dyn Searcher>,
        fetcher: Arc<dyn Fetcher>,
        runner: Arc<dyn Runner>,
        engine: Arc<dyn Inference>,
        models: Arc<dyn ModelSwitch>,
    ) -> Self {
        Self {
            searcher,
            fetcher,
            runner,
            engine,
            models,
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
}

impl ChatHandling for ChatUseCase {
    /// Zed-style agentic loop: the model may call tools before answering.
    async fn execute(&self, cmd: ChatCmd) -> Result<ChatOutcome, String> {
        let allow_tools = cmd.mode == SearchMode::Auto;

        let mut context = String::new();
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
