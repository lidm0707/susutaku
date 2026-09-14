//! Compound service: the chat use case. Depends on ports (traits) and the
//! domain, never on concrete infrastructure.

use std::sync::Arc;

use crate::domain::{
    BoardOp, BoardRequest, BoundRepo, ChatCmd, ChatOutcome, GenReply, GitOp, Prompt, SearchMode,
    SearchResult, TOOL_DENIED, TOOL_RESULT_HEADER, TOOL_ROUNDS_MAX, TOOL_SUMMARY_MAX, ToolCall,
    ToolKind, ToolSet, ToolUse,
};
use crate::port::inbound::ChatHandling;
use crate::port::outbound::{
    AgentConfigRepo, AgentGit, BoardOps, ChatMemory, Fetcher, Inference, ModelSwitch, ProjectGit,
    Runner, Searcher,
};
use susutaku_mlx::stats::GenStats;
use susutaku_mlx::tok::TokKind;

const MEMORY_RECALL_TOP_K: usize = 5;
const MEMORY_CONTEXT_HEADER: &str = "Earlier relevant conversation:\n";
const TOOL_ERROR: &str = "tool failed: ";
const TOOL_FALLBACK_NOTE: &str = "(the model could not finish this run; try again)";
const BOARD_ERROR_PREFIX: &str = "error:";

pub struct ChatUseCase {
    searcher: Arc<dyn Searcher>,
    fetcher: Arc<dyn Fetcher>,
    runner: Arc<dyn Runner>,
    engine: Arc<dyn Inference>,
    models: Arc<dyn ModelSwitch>,
    memory: Option<Arc<dyn ChatMemory>>,
    board: Arc<dyn BoardOps>,
    agents: Arc<dyn AgentConfigRepo>,
    /// Per-agent git host; `None` degrades git ops to the shared work tree.
    agent_git: Option<Arc<dyn AgentGit>>,
    /// Resolves the repo bound to the chat's project; `None` disables
    /// url-less GIT CLONE.
    project_git: Option<Arc<dyn ProjectGit>>,
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
            agent_git: None,
            project_git: None,
        }
    }

    /// Routes git toolcalls of named agents to their own work tree (via the
    /// manager); chats without an agent keep using the shared work tree.
    pub fn with_agent_git(mut self, agent_git: Arc<dyn AgentGit>) -> Self {
        self.agent_git = Some(agent_git);
        self
    }

    /// Lets a url-less GIT CLONE resolve the repo bound to the chat's
    /// project (thread → project → repo setting).
    pub fn with_project_git(mut self, project_git: Arc<dyn ProjectGit>) -> Self {
        self.project_git = Some(project_git);
        self
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
        image: Option<String>,
        max_tokens: usize,
        tok: TokKind,
        think: bool,
    ) -> Result<GenReply, String> {
        let rx = self
            .engine
            .submit_with_image(prompt, image, max_tokens, tok, think)?;
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

    fn outcome(
        &self,
        last_good: (String, String, GenStats),
        searched: bool,
        tools: Vec<ToolUse>,
        memories: Vec<String>,
    ) -> ChatOutcome {
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
            tools,
            memories,
            stats,
        }
    }

    /// Fills a url-less CLONE with the repo bound to the chat's project;
    /// returns the executable op plus the trace label (credential-free url).
    fn resolve_git_op(op: GitOp, bound: Option<&BoundRepo>) -> Result<(GitOp, String), String> {
        match op {
            GitOp::Clone {
                url: Some(url),
                token,
            } => Ok((
                GitOp::Clone {
                    url: Some(url.clone()),
                    token,
                },
                url,
            )),
            GitOp::Clone { token, .. } => match bound {
                Some(repo) => Ok((
                    GitOp::Clone {
                        url: Some(repo.url.clone()),
                        token: token.or_else(|| repo.secret.clone()),
                    },
                    repo.display_url.clone(),
                )),
                None => Err(
                    "no git repo is bound to this chat's project (bind one in settings → git repos)"
                        .to_string(),
                ),
            },
            GitOp::Status => Ok((GitOp::Status, "status".to_string())),
            GitOp::Diff => Ok((GitOp::Diff, "diff".to_string())),
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
    async fn recall_blocking(&self, thread: &str, query: &str) -> Vec<String> {
        let Some(memory) = self.memory.as_ref().map(Arc::clone) else {
            return Vec::new();
        };
        let owned = query.to_string();
        let thread = thread.to_string();
        let recalled = tokio::task::spawn_blocking(move || {
            memory.recall(&thread, &owned, MEMORY_RECALL_TOP_K)
        })
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();
        recalled
            .iter()
            .map(|hit| format!("{}: {}", hit.role, hit.text))
            .collect()
    }

    async fn remember_blocking(&self, thread: &str, role: &str, text: &str) {
        let Some(memory) = self.memory.as_ref().map(Arc::clone) else {
            return;
        };
        let (thread, role, text) = (thread.to_string(), role.to_string(), text.to_string());
        let _ = tokio::task::spawn_blocking(move || memory.remember(&thread, &role, &text)).await;
    }
}

impl ChatHandling for ChatUseCase {
    fn inference(&self) -> Option<Arc<dyn Inference>> {
        Some(self.engine.clone())
    }

    /// Zed-style agentic loop: the model may call tools before answering.
    async fn execute(&self, cmd: ChatCmd) -> Result<ChatOutcome, String> {
        let allow_tools = cmd.mode == SearchMode::Auto;
        let mut tools = self.agent_tools(cmd.agent.as_deref()).await;
        // Auto-select coding only when the work tree fits: a git repo must
        // exist, otherwise file writes have no project to belong to.
        if !self.runner.has_git_repo() {
            tools = tools.without_coding();
        }

        let mut context = String::new();
        let mut memories: Vec<String> = Vec::new();
        if let Some(thread) = cmd.thread_id.as_deref() {
            memories = self.recall_blocking(thread, &cmd.message).await;
            if !memories.is_empty() {
                context.push_str(MEMORY_CONTEXT_HEADER);
                for line in &memories {
                    context.push_str(line);
                    context.push('\n');
                }
            }
        }
        let bound = match (self.project_git.as_ref(), cmd.thread_id.as_deref()) {
            (Some(git), Some(thread)) if tools.allows(ToolKind::Git) => {
                git.bound_repo(thread).await
            }
            _ => None,
        };
        if let Some(repo) = &bound {
            context.push_str(&format!(
                "PROJECT GIT REPO: {} is bound to this project; TOOL: GIT CLONE without a url clones it.\n",
                repo.display_url
            ));
        }
        let mut trace: Vec<ToolUse> = Vec::new();
        let mut searched = false;
        if cmd.mode == SearchMode::Force {
            let results = self.search_blocking(&cmd.message).await?;
            searched = true;
            trace.push(ToolUse::new(
                ToolKind::Search,
                &cmd.message,
                &Ok(Prompt::format_results(&cmd.message, &results)),
            ));
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
            .infer(
                prompt,
                cmd.image.clone(),
                cmd.max_tokens,
                cmd.tokenizer,
                cmd.think,
            )
            .await?;
        let mut last_good = (reply.text.clone(), reply.model.clone(), reply.stats);

        let mut rounds = 0usize;
        while let Some(call) = self.next_call(&reply, allow_tools, rounds) {
            rounds += 1;
            match call {
                ToolCall::Search(query) if tools.allows(ToolKind::Search) => {
                    searched = true;
                    let result = self
                        .search_blocking(&query)
                        .await
                        .map(|results| Prompt::format_results(&cmd.message, &results));
                    trace.push(ToolUse::new(ToolKind::Search, &query, &result));
                    let note = result.unwrap_or_else(|e| format!("{TOOL_ERROR}{e}"));
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&note);
                }
                ToolCall::Fetch(url) if tools.allows(ToolKind::Fetch) => {
                    let result = self.fetch_blocking(&url).await;
                    trace.push(ToolUse::new(ToolKind::Fetch, &url, &result));
                    let page = result.unwrap_or_else(|e| format!("{TOOL_ERROR}{e}"));
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&page);
                }
                ToolCall::Shell(shell_cmd) if tools.allows(ToolKind::Shell) => {
                    let result = self.shell_blocking(&shell_cmd).await;
                    trace.push(ToolUse::new(ToolKind::Shell, &shell_cmd, &result));
                    let out = result.unwrap_or_else(|e| format!("{TOOL_ERROR}{e}"));
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&out);
                }
                ToolCall::Coding { path, code } if tools.allows(ToolKind::Coding) => {
                    let bytes = code.len();
                    let label = format!("write {path} ({bytes} bytes)");
                    let runner = Arc::clone(&self.runner);
                    let result =
                        tokio::task::spawn_blocking(move || runner.write_file(&path, &code))
                            .await
                            .map_err(|_| "coding task panicked".to_string())
                            .and_then(|inner| inner)
                            .map(|_| "file written".to_string());
                    trace.push(ToolUse::new(ToolKind::Coding, &label, &result));
                    let out = result.unwrap_or_else(|e| format!("{TOOL_ERROR}{e}"));
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&out);
                }
                ToolCall::Math(expr) if tools.allows(ToolKind::Math) => {
                    let result = math::geomath::eval(&expr);
                    trace.push(ToolUse::new(ToolKind::Math, &expr, &result));
                    let out = result.unwrap_or_else(|e| format!("{TOOL_ERROR}{e}"));
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&out);
                }
                ToolCall::Git(op) if tools.allows(ToolKind::Git) => {
                    let agent = cmd.agent.as_deref().unwrap_or("").trim().to_string();
                    let (input, result) = match Self::resolve_git_op(op, bound.as_ref()) {
                        Err(e) => ("clone".to_string(), Err(e)),
                        Ok((op, input)) => {
                            let result = match (self.agent_git.as_ref(), !agent.is_empty()) {
                                (Some(git), true) => {
                                    let git = Arc::clone(git);
                                    tokio::task::spawn_blocking(move || git.git_for(&agent, &op))
                                        .await
                                        .map_err(|_| "git task panicked".to_string())
                                        .and_then(|inner| inner)
                                }
                                _ => {
                                    let runner = Arc::clone(&self.runner);
                                    tokio::task::spawn_blocking(move || runner.git(&op))
                                        .await
                                        .map_err(|_| "git task panicked".to_string())
                                        .and_then(|inner| inner)
                                }
                            };
                            (input, result)
                        }
                    };
                    trace.push(ToolUse::new(ToolKind::Git, &input, &result));
                    let out = result.unwrap_or_else(|e| format!("{TOOL_ERROR}{e}"));
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&out);
                }
                call if Self::board_op(&call).is_some() && tools.allows(ToolKind::Board) => {
                    let token = cmd.board_token.clone();
                    let op = Self::board_op(&call).expect("matched guard");
                    let out = self.board_blocking(token, op).await;
                    let ok = !out.starts_with(BOARD_ERROR_PREFIX);
                    trace.push(ToolUse {
                        kind: ToolKind::Board,
                        input: String::new(),
                        ok,
                        summary: out.chars().take(TOOL_SUMMARY_MAX).collect(),
                    });
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&out);
                }
                _ => {
                    trace.push(ToolUse::denied(ToolKind::Search, ""));
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
                .infer(prompt, None, cmd.max_tokens, cmd.tokenizer, cmd.think)
                .await
            {
                Ok(r) => {
                    last_good = (r.text.clone(), r.model.clone(), r.stats);
                    r
                }
                Err(e) => {
                    tracing::warn!("chat tool round {rounds} inference failed: {e}");
                    return Ok(self.outcome(last_good, searched, trace, memories));
                }
            };
        }

        // Tool budget exhausted while the model still wants a tool: force a
        // final answer on the gathered context, without the tool offer.
        if allow_tools && rounds > 0 && ToolCall::parse(&reply.text).is_some() {
            reply = match self
                .infer(
                    Prompt::build(&cmd.message, &context, false, false, &tools),
                    None,
                    cmd.max_tokens,
                    cmd.tokenizer,
                    cmd.think,
                )
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("chat final answer inference failed: {e}");
                    return Ok(self.outcome(last_good, searched, trace, memories));
                }
            };
        }

        if let Some(thread) = cmd.thread_id.as_deref() {
            self.remember_blocking(thread, "user", &cmd.message).await;
            self.remember_blocking(thread, "assistant", &reply.text)
                .await;
        }

        Ok(self.outcome(
            (reply.text, reply.model, reply.stats),
            searched,
            trace,
            memories,
        ))
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
