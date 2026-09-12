//! Compound service: the chat use case. Depends on ports (traits) and the
//! domain, never on concrete infrastructure.

use std::sync::Arc;

use crate::domain::{
    BoardOp, BoardRequest, ChatCmd, ChatOutcome, GenReply, Prompt, SearchMode, SearchResult,
    TOOL_DENIED, TOOL_RESULT_HEADER, TOOL_ROUNDS_MAX, ToolCall, ToolKind, ToolSet,
};
use crate::port::inbound::ChatHandling;
use crate::port::outbound::{
    AgentConfigRepo, BoardOps, ChatMemory, Fetcher, Inference, ModelSwitch, Runner, Searcher,
};
use susutaku_mlx::stats::GenStats;
use susutaku_mlx::tok::TokKind;

const MEMORY_RECALL_TOP_K: usize = 5;
const MEMORY_CONTEXT_HEADER: &str = "Earlier relevant conversation:\n";
const TOOL_ERROR: &str = "tool failed: ";
const TOOL_FALLBACK_NOTE: &str = "(the model could not finish this run; try again)";

pub struct ChatUseCase {
    searcher: Arc<dyn Searcher>,
    fetcher: Arc<dyn Fetcher>,
    runner: Arc<dyn Runner>,
    engine: Arc<dyn Inference>,
    models: Arc<dyn ModelSwitch>,
    memory: Option<Arc<dyn ChatMemory>>,
    board: Arc<dyn BoardOps>,
    agents: Arc<dyn AgentConfigRepo>,
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
        board: Arc<dyn BoardOps>,
        agents: Arc<dyn AgentConfigRepo>,
    ) -> Self {
        Self {
            searcher,
            fetcher,
            runner,
            engine,
            models,
            memory,
            board,
            agents,
        }
    }

    /// Resolves the named agent's tool allow-list; unknown/unnamed agents get
    /// the full set. Store errors degrade to the full set (fail open).
    async fn agent_tools(&self, agent: Option<&str>) -> ToolSet {
        let Some(name) = agent.map(str::trim).filter(|n| !n.is_empty()) else {
            return ToolSet::all();
        };
        match self.agents.by_name(name).await {
            Ok(Some(cfg)) => ToolSet::from_names(&cfg.allowed_tools).unwrap_or_default(),
            _ => ToolSet::all(),
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

    fn outcome(&self, last_good: (String, String, GenStats), searched: bool) -> ChatOutcome {
        let (text, model, stats) = last_good;
        let text = if ToolCall::parse(&text).is_some() {
            TOOL_FALLBACK_NOTE.to_string()
        } else {
            text
        };
        ChatOutcome {
            model: Some(model),
            text,
            searched,
            stats,
        }
    }

    fn board_op(call: &ToolCall) -> Option<BoardOp> {
        match call {
            ToolCall::PipelineCreate { name, spec } => Some(BoardOp::CreatePipeline {
                name: name.clone(),
                spec: spec.clone(),
            }),
            ToolCall::CardCreate { project_id, title } => Some(BoardOp::CreateCard {
                project_id: *project_id,
                title: title.clone(),
            }),
            ToolCall::CardLink {
                card_id,
                pipeline_id,
            } => Some(BoardOp::LinkPipeline {
                card_id: *card_id,
                pipeline_id: *pipeline_id,
            }),
            ToolCall::CardSchedule { card_id, cron } => Some(BoardOp::SetCron {
                card_id: *card_id,
                cron: Some(cron.clone()),
            }),
            ToolCall::BoardList => Some(BoardOp::Summary),
            _ => None,
        }
    }

    async fn board_blocking(&self, token: Option<String>, op: BoardOp) -> String {
        match self.board.exec(BoardRequest { token, op }).await {
            Ok(out) => out,
            Err(e) => format!("error: {e}"),
        }
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
        let tools = self.agent_tools(cmd.agent.as_deref()).await;

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

        let mut prompt = Prompt::build(
            &cmd.message,
            &context,
            allow_tools,
            cmd.board_token.is_some(),
            &tools,
        );
        let mut reply = self
            .infer(prompt, cmd.max_tokens, cmd.tokenizer, cmd.think)
            .await?;
        let mut last_good = (reply.text.clone(), reply.model.clone(), reply.stats);

        let mut rounds = 0usize;
        while let Some(call) = self.next_call(&reply, allow_tools, rounds) {
            rounds += 1;
            match call {
                ToolCall::Search(query) if tools.allows(ToolKind::Search) => {
                    searched = true;
                    let note = match self.search_blocking(&query).await {
                        Ok(results) => Prompt::format_results(&cmd.message, &results),
                        Err(e) => format!("{TOOL_ERROR}{e}"),
                    };
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&note);
                }
                ToolCall::Fetch(url) if tools.allows(ToolKind::Fetch) => {
                    let page = match self.fetch_blocking(&url).await {
                        Ok(page) => page,
                        Err(e) => format!("{TOOL_ERROR}{e}"),
                    };
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&page);
                }
                ToolCall::Shell(cmd) if tools.allows(ToolKind::Shell) => {
                    let out = match self.shell_blocking(&cmd).await {
                        Ok(out) => out,
                        Err(e) => format!("{TOOL_ERROR}{e}"),
                    };
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&out);
                }
                call if Self::board_op(&call).is_some() && tools.allows(ToolKind::Board) => {
                    let token = cmd.board_token.clone();
                    let op = Self::board_op(&call).expect("matched guard");
                    let out = self.board_blocking(token, op).await;
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&out);
                }
                _ => {
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(TOOL_DENIED);
                }
            }
            prompt = Prompt::build(
                &cmd.message,
                &context,
                allow_tools,
                cmd.board_token.is_some(),
                &tools,
            );
            reply = match self
                .infer(prompt, cmd.max_tokens, cmd.tokenizer, cmd.think)
                .await
            {
                Ok(r) => {
                    last_good = (r.text.clone(), r.model.clone(), r.stats);
                    r
                }
                Err(e) => {
                    tracing::warn!("chat tool round {rounds} inference failed: {e}");
                    return Ok(self.outcome(last_good, searched));
                }
            };
        }

        // Tool budget exhausted while the model still wants a tool: force a
        // final answer on the gathered context, without the tool offer.
        if allow_tools && rounds > 0 && ToolCall::parse(&reply.text).is_some() {
            reply = match self
                .infer(
                    Prompt::build(&cmd.message, &context, false, false, &tools),
                    cmd.max_tokens,
                    cmd.tokenizer,
                    cmd.think,
                )
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("chat final answer inference failed: {e}");
                    return Ok(self.outcome(last_good, searched));
                }
            };
        }

        self.remember_blocking("user", &cmd.message).await;
        self.remember_blocking("assistant", &reply.text).await;

        Ok(self.outcome((reply.text, reply.model, reply.stats), searched))
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
