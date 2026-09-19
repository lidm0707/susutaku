//! Compound service: the chat use case. Depends on ports (traits) and the
//! domain, never on concrete infrastructure.

use std::sync::Arc;

use crate::domain::{
    ArtifactKind, BoardOp, BoardRequest, BoundRepo, ChatCmd, ChatOutcome, GenReply, GitOp,
    INTERRUPTED_NOTE, LspOp, Prompt, ResourceService, SearchMode, SearchResult, SkillService,
    TOOL_DENIED, TOOL_RESULT_HEADER, TOOL_ROUNDS_MAX, TOOL_SUMMARY_MAX, ToolCall, ToolEvent,
    ToolKind, ToolSet, ToolUse,
};
use crate::port::inbound::ChatHandling;
use crate::port::outbound::ModelEngines;
use crate::port::outbound::{
    AgentConfigRepo, AgentGit, AgentRun, BoardOps, ChatMemory, Fetcher, Inference, ModelSwitch,
    ProjectGit, Runner, Searcher, ThreadEnvs,
};
use susutaku_mlx::stats::GenStats;
use susutaku_mlx::tok::TokKind;
use task_rs::ThinkLevel;
use task_rs::resource::UpsertResource;

const MEMORY_RECALL_TOP_K: usize = 5;
const MEMORY_CONTEXT_HEADER: &str = "Earlier relevant conversation:\n";
const PROJECT_SKILLS_HEADER: &str =
    "PROJECT SKILLS (how to work in this project; follow when relevant):\n";
const THINK_HINT_LOW: &str = "THINKING: think briefly before answering; keep reasoning short.\n";
const THINK_HINT_MEDIUM: &str =
    "THINKING: reason through the problem step by step before answering.\n";
const THINK_HINT_HIGH: &str =
    "THINKING: reason deeply — consider alternatives and edge cases, then answer.\n";

/// Context hint expressing the agent's custom thinking depth; off = none.
fn think_hint(level: ThinkLevel) -> Option<&'static str> {
    match level {
        ThinkLevel::Off => None,
        ThinkLevel::Low => Some(THINK_HINT_LOW),
        ThinkLevel::Medium => Some(THINK_HINT_MEDIUM),
        ThinkLevel::High => Some(THINK_HINT_HIGH),
    }
}
const TOOL_ERROR: &str = "tool failed: ";
const TOOL_FALLBACK_NOTE: &str = "(the model could not finish this run; try again)";
const TOOL_MALFORMED: &str = "malformed tool line: nothing was run. The format is exactly `TOOL: KIND arg` — e.g. `TOOL: CARD_CREATE <numeric project_id> <short title> | <description>`. Ids are numbers: call BOARD_LIST (or CARD_FIND) first when you don't know them, then repeat the tool line.";

pub const AUTO_TITLE_MAX_CHARS: usize = 60;
pub const AUTO_DESC_MAX_CHARS: usize = 2000;
pub const CLASSIFY_MAX_TOKENS: usize = 8;
pub const CLASSIFY_YES: &str = "YES";
pub const CLASSIFY_PROMPT: &str = "You are a strict YES/NO classifier. USER MESSAGE:\n";
pub const CLASSIFY_QUESTION: &str = "\n\nDoes the user ask to do, build, change, fix, or create work (an action for the agent), or only ask a question / discuss? Answer with exactly one word: YES (action) or NO (question).";
const CARD_ID_PREFIX: &str = "card ";
const AUTO_TITLE_FALLBACK: &str = "task";
const AUTO_REPLY_OPEN: &str = "opened ";
const AUTO_RUN_NOTE: &str = "run: ";
const BOARD_ERROR_PREFIX: &str = "error:";
/// At most this many files are picked up from one shell output.
const ARTIFACT_SCAN_MAX: usize = 8;
/// Images larger than this are linked by path, not inlined as a data URL.
const RESOURCE_IMAGE_MAX_BYTES: u64 = 4 * 1024 * 1024;
/// Text resources are truncated to this many characters.
const RESOURCE_TEXT_MAX_CHARS: usize = 64 * 1024;

/// Base64 data-URL mime for an image extension.
fn image_mime(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    }
}

fn use_base64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub struct ChatUseCase {
    searcher: Arc<dyn Searcher>,
    fetcher: Arc<dyn Fetcher>,
    runner: Arc<dyn Runner>,
    engine: Arc<dyn Inference>,
    models: Arc<dyn ModelSwitch>,
    memory: Option<Arc<dyn ChatMemory>>,
    board: Arc<dyn BoardOps>,
    agents: Arc<dyn AgentConfigRepo>,
    /// Card-resource store; when present (with a card id on the turn),
    /// artifacts produced by tools are attached to the target card.
    resources: Option<Arc<ResourceService>>,
    /// Per-agent git host; `None` degrades git ops to the shared work tree.
    agent_git: Option<Arc<dyn AgentGit>>,
    /// Runs commands inside a named agent's own sandbox; `None` disables
    /// the AGENT_RUN tool.
    agent_run: Option<Arc<dyn AgentRun>>,
    /// Per-thread sandbox environments; `None` keeps every thread on the
    /// shared work tree.
    thread_envs: Option<Arc<dyn ThreadEnvs>>,
    /// Resolves the repo bound to the chat's project; `None` disables
    /// url-less GIT CLONE.
    project_git: Option<Arc<dyn ProjectGit>>,
    /// Per-agent skill sheets embedded into the prompt; `None` embeds none.
    skills: Option<Arc<SkillService>>,
    /// Live tool-progress sink for streamed turns; `None` emits nothing.
    tools_tx: Option<tokio::sync::broadcast::Sender<ToolEvent>>,
    /// Routes a mentioned agent's configured cloud model to its engine;
    /// `None` (or an unknown model) keeps every turn on the shared engine.
    engines: Option<Arc<dyn ModelEngines>>,
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
            resources: None,
            agent_git: None,
            agent_run: None,
            thread_envs: None,
            project_git: None,
            skills: None,
            tools_tx: None,
            engines: None,
        }
    }

    /// Enable per-agent model routing (Z.ai cloud engines).
    pub fn with_engines(mut self, engines: Arc<dyn ModelEngines>) -> Self {
        self.engines = Some(engines);
        self
    }

    /// The engine serving a mentioned agent: its configured model when the
    /// router knows it, else the shared engine. Card-comment mentions must
    /// reach the same cloud model the card run would use — without this they
    /// land on the shared engine (the e2e mock in the deploy stack) and the
    /// agent answers in canned mock lines.
    async fn agent_engine(&self, agent: Option<&str>) -> Arc<dyn Inference> {
        let routed = match (self.engines.as_ref(), agent) {
            (Some(engines), Some(name)) => {
                let cfg = self.agents.by_name(name.trim()).await.ok().flatten();
                match cfg {
                    Some(cfg) if !cfg.model.trim().is_empty() => {
                        engines.engine_for(cfg.model.trim())
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        routed.unwrap_or_else(|| self.engine.clone())
    }

    /// Stream one event per completed tool call to the UI while the loop runs.
    pub fn with_tool_events(mut self, tx: tokio::sync::broadcast::Sender<ToolEvent>) -> Self {
        self.tools_tx = Some(tx);
        self
    }

    /// Enable artifact persistence: tool-produced files land as resources on
    /// the card named by the turn's `card_id`.
    pub fn with_resources(mut self, resources: Arc<ResourceService>) -> Self {
        self.resources = Some(resources);
        self
    }

    /// Routes git toolcalls of named agents to their own work tree (via the
    /// manager); chats without an agent keep using the shared work tree.
    pub fn with_agent_git(mut self, agent_git: Arc<dyn AgentGit>) -> Self {
        self.agent_git = Some(agent_git);
        self
    }

    /// Lets AGENT_RUN drive a named agent's own sandbox via the manager.
    pub fn with_agent_run(mut self, agent_run: Arc<dyn AgentRun>) -> Self {
        self.agent_run = Some(agent_run);
        self
    }

    /// Routes a coding thread's shell/coding/lsp tools into its own sandbox.
    pub fn with_thread_envs(mut self, thread_envs: Arc<dyn ThreadEnvs>) -> Self {
        self.thread_envs = Some(thread_envs);
        self
    }

    /// The runner this turn's tools use: the thread's own environment when
    /// one exists (or can be created), else the shared work tree.
    fn turn_runner(&self, thread_id: Option<&str>, agent: Option<&str>) -> Arc<dyn Runner> {
        if let Some(envs) = self.thread_envs.as_ref()
            && let Some(tid) = thread_id
            && let Ok(runner) = envs.runner_for(tid, agent)
        {
            return runner;
        }
        Arc::clone(&self.runner)
    }

    /// Lets a url-less GIT CLONE resolve the repo bound to the chat's
    /// project (thread → project → repo setting).
    pub fn with_project_git(mut self, project_git: Arc<dyn ProjectGit>) -> Self {
        self.project_git = Some(project_git);
        self
    }

    /// Embeds the skill sheets attached to the named agent into every prompt.
    pub fn with_skills(mut self, skills: Arc<SkillService>) -> Self {
        self.skills = Some(skills);
        self
    }

    /// Skill sheets attached to the named agent, formatted for the prompt.
    /// Unknown agent / no repo / store errors degrade to empty (fail open).
    async fn agent_skills(&self, agent: Option<&str>) -> String {
        let Some(skills) = self.skills.as_ref() else {
            return String::new();
        };
        let named = agent.map(str::trim).filter(|n| !n.is_empty());
        let agent = match named {
            Some(name) => self.agents.by_name(name).await.ok().flatten(),
            None => None,
        };
        let Some(cfg) = agent else {
            return String::new();
        };
        match skills.list_for_agent(cfg.id).await {
            Ok(rows) => rows
                .iter()
                .map(|s| format!("### {}\n{}\n", s.name, s.body))
                .collect(),
            Err(_) => String::new(),
        }
    }

    /// Resolves the named agent's tool allow-list; unknown/unnamed agents get
    /// the full set. Store errors degrade to the full set (fail open).
    async fn agent_tools(&self, agent: Option<&str>) -> ToolSet {
        let Some(name) = agent.map(str::trim).filter(|n| !n.is_empty()) else {
            return ToolSet::all();
        };
        match self.agents.by_name(name).await {
            Ok(Some(cfg)) => ToolSet::from_names(&cfg.allowed_tools).unwrap_or_else(|_| {
                // stale/unknown tool names in the stored list must not mute
                // the agent entirely — deny nothing, fail open
                ToolSet::all()
            }),
            _ => ToolSet::all(),
        }
    }

    /// Custom reasoning depth configured on the named agent; off/unknown = Off.
    async fn agent_think_level(&self, agent: Option<&str>) -> ThinkLevel {
        let Some(name) = agent.map(str::trim).filter(|n| !n.is_empty()) else {
            return ThinkLevel::default();
        };
        match self.agents.by_name(name).await {
            Ok(Some(cfg)) => cfg.think_level(),
            _ => ThinkLevel::default(),
        }
    }

    fn next_call(&self, reply: &GenReply, allow_tools: bool, rounds: usize) -> Option<ToolCall> {
        if !allow_tools || rounds >= TOOL_ROUNDS_MAX {
            return None;
        }
        // an interrupted partial reply never acts on tools
        if reply.text.contains(INTERRUPTED_NOTE) {
            return None;
        }
        ToolCall::parse(&reply.text)
    }

    /// A tool line was offered but does not parse: one corrective turn while
    /// budget remains, instead of dead-ending on the raw line.
    fn malformed_tool(&self, reply: &GenReply, allow_tools: bool, rounds: usize) -> bool {
        allow_tools && rounds < TOOL_ROUNDS_MAX && ToolCall::offers(&reply.text)
    }

    /// Deterministic work-request fallback (user option 2): the chat's agent
    /// is bound, the model answered without any tool call — classify the
    /// message once and, for a do/build/change request, open a card in the
    /// chat's project, assign the agent and run it. Pure questions keep the
    /// model's normal answer.
    async fn auto_card(
        &self,
        cmd: &ChatCmd,
        allow_tools: bool,
        rounds: usize,
        reply: &GenReply,
    ) -> Option<String> {
        let agent = cmd
            .agent
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())?;
        let token = cmd.board_token.clone()?;
        let project_id = cmd.project_id.filter(|id| *id > 0)?;
        if !allow_tools || rounds > 0 || ToolCall::offers(&reply.text) {
            return None;
        }
        // an interrupted partial is not a finished answer: keep it as-is
        if reply.text.contains(INTERRUPTED_NOTE) {
            return None;
        }
        if !self.is_work_request(&cmd.message, cmd.tokenizer).await {
            return None;
        }
        let create = BoardOp::CreateCard {
            project_id,
            title: short_title(&cmd.message),
            description: Some(cmd.message.chars().take(AUTO_DESC_MAX_CHARS).collect()),
        };
        let created = self.board_blocking(Some(token.clone()), create).await;
        let card_id = parse_card_id(&created)?;
        self.board_blocking(
            Some(token.clone()),
            BoardOp::AssignAgent {
                card_id,
                agent: agent.to_owned(),
            },
        )
        .await;
        let run = self
            .board_blocking(Some(token), BoardOp::RunCard { card_id })
            .await;
        Some(format!(
            "{AUTO_REPLY_OPEN}card {card_id} for agent {agent} and ran it:\n{run}"
        ))
    }

    /// One tiny YES/NO inference: does the message ask for work?
    async fn is_work_request(&self, message: &str, tok: TokKind) -> bool {
        let prompt = format!("{CLASSIFY_PROMPT}{message}{CLASSIFY_QUESTION}");
        match self
            .infer(&self.engine, prompt, None, CLASSIFY_MAX_TOKENS, tok, false)
            .await
        {
            Ok(r) => r.text.trim().to_uppercase().starts_with(CLASSIFY_YES),
            Err(_) => false,
        }
    }

    async fn infer(
        &self,
        engine: &Arc<dyn Inference>,
        prompt: String,
        image: Option<String>,
        max_tokens: usize,
        tok: TokKind,
        think: bool,
    ) -> Result<GenReply, String> {
        let rx = engine.submit_with_image(prompt, image, max_tokens, tok, think)?;
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

    async fn shell_blocking_on(
        &self,
        runner: Arc<dyn Runner>,
        cmd: &str,
    ) -> Result<String, String> {
        let owned = cmd.to_string();
        tokio::task::spawn_blocking(move || runner.run(&owned))
            .await
            .map_err(|_| "shell task panicked".to_string())?
    }

    /// LSP runs host-side: read the file text from the work tree, then query
    /// rust-analyzer rooted at the work tree. One-shot session per call.
    async fn lsp_blocking(
        &self,
        runner: Arc<dyn Runner>,
        op: LspOp,
        path: &str,
        line: u32,
        col: usize,
    ) -> Result<String, String> {
        let path = path.to_string();
        tokio::task::spawn_blocking(move || {
            let text = runner.read_file(&path)?;
            let abs = runner
                .workspace_root()
                .join(&path)
                .to_string_lossy()
                .into_owned();
            let root = runner.workspace_root().to_string_lossy().into_owned();
            match op {
                LspOp::Definition => {
                    core_agent::toolcall::lsp::definition(&root, &abs, &text, line, col)
                }
                LspOp::References => {
                    core_agent::toolcall::lsp::references(&root, &abs, &text, line, col)
                }
                LspOp::Hover => core_agent::toolcall::lsp::hover(&root, &abs, &text, line, col),
            }
        })
        .await
        .map_err(|_| "lsp task panicked".to_string())?
    }

    /// Build the turn outcome, first attaching any produced artifacts to the
    /// target card (no-op without card id or resource store).
    async fn finish(
        &self,
        runner: &Arc<dyn Runner>,
        cmd: &ChatCmd,
        last_good: (String, String, GenStats),
        searched: bool,
        tools: Vec<ToolUse>,
        memories: Vec<String>,
    ) -> ChatOutcome {
        if let Some(card_id) = cmd.card_id {
            self.persist_artifacts(runner, card_id, &tools).await;
        }
        self.outcome(last_good, searched, tools, memories)
    }

    /// Files a successful shell call left behind: tokens in its output that
    /// exist under the workspace root with a known image/text extension.
    fn scan_artifacts(&self, runner: &Arc<dyn Runner>, output: &str) -> Vec<String> {
        let root = runner.workspace_root();
        let mut found: Vec<String> = Vec::new();
        for token in output.split(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
            if token.is_empty()
                || found.contains(&token.to_string())
                || ArtifactKind::from_path(token) == ArtifactKind::File
            {
                continue;
            }
            if root.join(token).is_file() {
                found.push(token.to_string());
                if found.len() >= ARTIFACT_SCAN_MAX {
                    break;
                }
            }
        }
        found
    }

    /// Upsert every tool artifact as a card resource: images inline as data
    /// URLs (size-capped), text truncated, other files stored by path.
    async fn persist_artifacts(&self, runner: &Arc<dyn Runner>, card_id: i64, tools: &[ToolUse]) {
        let Some(resources) = self.resources.as_ref() else {
            return;
        };
        let root = runner.workspace_root();
        for artifact in tools.iter().flat_map(|t| &t.artifacts) {
            let full = root.join(&artifact.path);
            let Some(name) = full.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let content = match artifact.kind {
                ArtifactKind::Image => {
                    let Ok(meta) = std::fs::metadata(&full) else {
                        continue;
                    };
                    if meta.len() > RESOURCE_IMAGE_MAX_BYTES {
                        artifact.path.clone()
                    } else if let Ok(bytes) = std::fs::read(&full) {
                        let ext = full
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or_default();
                        format!("data:{};base64,{}", image_mime(ext), use_base64(&bytes))
                    } else {
                        continue;
                    }
                }
                ArtifactKind::Text => std::fs::read_to_string(&full)
                    .map(|s| s.chars().take(RESOURCE_TEXT_MAX_CHARS).collect())
                    .unwrap_or_else(|_| artifact.path.clone()),
                ArtifactKind::File => artifact.path.clone(),
            };
            if let Err(e) = resources
                .upsert(UpsertResource {
                    card_id,
                    name,
                    content: &content,
                })
                .await
            {
                tracing::warn!("artifact {name} not attached to card {card_id}: {e}");
            }
        }
    }

    fn outcome(
        &self,
        last_good: (String, String, GenStats),
        searched: bool,
        tools: Vec<ToolUse>,
        memories: Vec<String>,
    ) -> ChatOutcome {
        let (text, model, stats) = last_good;
        let text = if ToolCall::offers(&text) {
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
            GitOp::Push { branch, url, token } => {
                let (url, token, label) = Self::bind_repo(url, token, bound)?;
                let label = format!("push {branch} → {label}");
                Ok((GitOp::Push { branch, url, token }, label))
            }
            GitOp::PullRequest {
                title,
                head,
                base,
                url,
                token,
            } => {
                let (url, token, label) = Self::bind_repo(url, token, bound)?;
                let label = format!("pr “{title}” → {label}");
                Ok((
                    GitOp::PullRequest {
                        title,
                        head,
                        base,
                        url,
                        token,
                    },
                    label,
                ))
            }
            op @ (GitOp::Branch { .. } | GitOp::Commit { .. }) => {
                let label = match &op {
                    GitOp::Branch { name } => format!("branch {name}"),
                    GitOp::Commit { message } => format!("commit “{message}”"),
                    _ => unreachable!("matched above"),
                };
                Ok((op, label))
            }
            // Publish automation only — never produced by chat tool parsing.
            GitOp::TaskBranch { .. } => {
                Err("task branch reconcile is run automation only".to_string())
            }
        }
    }

    /// Resolves url/token for an op against the project-bound repo, so the
    /// model never carries credentials.
    fn bind_repo(
        url: Option<String>,
        token: Option<String>,
        bound: Option<&BoundRepo>,
    ) -> Result<(Option<String>, Option<String>, String), String> {
        match (url, bound) {
            (Some(url), _) => Ok((Some(url.clone()), token, url)),
            (None, Some(repo)) => Ok((
                Some(repo.url.clone()),
                token.or_else(|| repo.secret.clone()),
                repo.display_url.clone(),
            )),
            (None, None) => Err(
                "no git repo is bound to this chat's project (bind one in settings → git repos)"
                    .to_string(),
            ),
        }
    }

    fn board_op(call: &ToolCall) -> Option<BoardOp> {
        match call {
            ToolCall::CardCreate {
                project_id,
                title,
                description,
            } => Some(BoardOp::CreateCard {
                project_id: *project_id,
                title: title.clone(),
                description: description.clone(),
            }),
            ToolCall::CardAgent { card_id, agent } => Some(BoardOp::AssignAgent {
                card_id: *card_id,
                agent: agent.clone(),
            }),
            ToolCall::CardImage { card_id, image } => Some(BoardOp::SetImage {
                card_id: *card_id,
                image: image.clone(),
            }),
            ToolCall::CardSchedule { card_id, cron } => Some(BoardOp::SetCron {
                card_id: *card_id,
                cron: Some(cron.clone()),
            }),
            ToolCall::CardRoutineClear { card_id } => Some(BoardOp::SetCron {
                card_id: *card_id,
                cron: None,
            }),
            ToolCall::CardRun { card_id } => Some(BoardOp::RunCard { card_id: *card_id }),
            ToolCall::BoardList => Some(BoardOp::Summary),
            ToolCall::CardFind { query } => Some(BoardOp::FindCards {
                query: query.clone(),
            }),
            _ => None,
        }
    }

    /// Which permission kind this board toolcall needs.
    fn board_kind(call: &ToolCall) -> Option<ToolKind> {
        match call {
            ToolCall::BoardList | ToolCall::CardFind { .. } => Some(ToolKind::Board),
            ToolCall::CardCreate { .. }
            | ToolCall::CardAgent { .. }
            | ToolCall::CardImage { .. }
            | ToolCall::CardRun { .. } => Some(ToolKind::Card),
            ToolCall::CardSchedule { .. } | ToolCall::CardRoutineClear { .. } => {
                Some(ToolKind::Routine)
            }
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

/// First words of the message, capped — the auto-card title.
fn short_title(message: &str) -> String {
    let mut title = String::new();
    for word in message.split_whitespace() {
        if title.chars().count() + word.chars().count() + 1 > AUTO_TITLE_MAX_CHARS {
            break;
        }
        if !title.is_empty() {
            title.push(' ');
        }
        title.push_str(word);
    }
    if title.is_empty() {
        AUTO_TITLE_FALLBACK.to_owned()
    } else {
        title
    }
}

/// `card 12 created …` → 12 (the board's CreateCard reply format).
fn parse_card_id(created: &str) -> Option<i64> {
    created
        .strip_prefix(CARD_ID_PREFIX)
        .and_then(|rest| rest.split_once(' '))
        .and_then(|(id, _)| id.parse().ok())
}

impl ChatHandling for ChatUseCase {
    fn inference(&self) -> Option<Arc<dyn Inference>> {
        Some(self.engine.clone())
    }

    /// Zed-style agentic loop: the model may call tools before answering.
    async fn execute(&self, cmd: ChatCmd) -> Result<ChatOutcome, String> {
        let allow_tools = cmd.mode == SearchMode::Auto;
        let think_level = self.agent_think_level(cmd.agent.as_deref()).await;
        // Agent-configured thinking joins the per-request toggle: either one
        // turns the reasoning block on.
        let think = cmd.think || think_level.on();
        let mut tools = self.agent_tools(cmd.agent.as_deref()).await;
        // Per-thread sandbox: coding threads get their own environment.
        let base_runner = self.turn_runner(cmd.thread_id.as_deref(), cmd.agent.as_deref());
        // Auto-select coding only when the work tree fits: a git repo must
        // exist, otherwise file writes have no project to belong to.
        if !base_runner.has_git_repo() {
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
        let agent_skills = self.agent_skills(cmd.agent.as_deref()).await;
        // The bound agent's name must reach the model: CARD_AGENT needs it and
        // otherwise the model stalls asking "which agent?".
        if let Some(name) = cmd
            .agent
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            context.push_str(&format!(
                "CHAT AGENT: {name} is bound to this chat — assign it to new cards with CARD_AGENT unless the user names another agent.\n"
            ));
        }
        if !agent_skills.is_empty() {
            context.push_str(PROJECT_SKILLS_HEADER);
            context.push_str(&agent_skills);
        }
        // Task snapshot: ground the turn on the card's state + comment
        // thread, so a mention reply continues where the last one left off.
        if let Some(card_id) = cmd.card_id {
            if let Some(snap) = self.board.card_context(card_id).await {
                context.push_str(&snap);
                context.push('\n');
            }
        }
        if let Some(hint) = think_hint(think_level) {
            context.push_str(hint);
            context.push('\n');
        }
        let mut trace: Vec<ToolUse> = Vec::new();
        // card-flow progress across this turn's board tools
        let mut created_card: Option<i64> = None;
        let mut card_assigned = false;
        let mut card_ran = false;
        let mut card_scheduled = false;
        // stream every finished tool call to the UI as it happens
        let emit_tool = |u: &ToolUse| {
            if let Some(tx) = &self.tools_tx {
                let _ = tx.send(ToolEvent::from(u));
            }
        };
        let mut searched = false;
        if cmd.mode == SearchMode::Force {
            let results = self.search_blocking(&cmd.message).await?;
            searched = true;
            trace.push(ToolUse::new(
                ToolKind::Search,
                &cmd.message,
                &Ok(Prompt::format_results(&cmd.message, &results)),
            ));
            emit_tool(trace.last().expect("just pushed"));
            context.push_str(&Prompt::format_results(&cmd.message, &results));
        }

        let mut prompt = Prompt::build(
            &cmd.message,
            &context,
            allow_tools,
            cmd.board_token.is_some(),
            &tools,
        );
        let engine = self.agent_engine(cmd.agent.as_deref()).await;
        let mut reply = self
            .infer(
                &engine,
                prompt,
                cmd.image.clone(),
                cmd.max_tokens,
                cmd.tokenizer,
                think,
            )
            .await?;
        let mut last_good = (reply.text.clone(), reply.model.clone(), reply.stats);

        let mut rounds = 0usize;
        loop {
            let call = match self.next_call(&reply, allow_tools, rounds) {
                Some(call) => call,
                None if self.malformed_tool(&reply, allow_tools, rounds) => {
                    rounds += 1;
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(TOOL_MALFORMED);
                    prompt = Prompt::build(
                        &cmd.message,
                        &context,
                        allow_tools,
                        cmd.board_token.is_some(),
                        &tools,
                    );
                    reply = match self
                        .infer(&engine, prompt, None, cmd.max_tokens, cmd.tokenizer, think)
                        .await
                    {
                        Ok(r) => {
                            last_good = (r.text.clone(), r.model.clone(), r.stats);
                            r
                        }
                        Err(e) => {
                            tracing::warn!("chat malformed-tool retry inference failed: {e}");
                            return Ok(self
                                .finish(&base_runner, &cmd, last_good, searched, trace, memories)
                                .await);
                        }
                    };
                    continue;
                }
                None => break,
            };
            rounds += 1;
            let runner = base_runner.clone();
            match call {
                ToolCall::Search(query) if tools.allows(ToolKind::Search) => {
                    searched = true;
                    let result = self
                        .search_blocking(&query)
                        .await
                        .map(|results| Prompt::format_results(&cmd.message, &results));
                    trace.push(ToolUse::new(ToolKind::Search, &query, &result));
                    emit_tool(trace.last().expect("just pushed"));
                    let note = result.unwrap_or_else(|e| format!("{TOOL_ERROR}{e}"));
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&note);
                }
                ToolCall::Fetch(url) if tools.allows(ToolKind::Fetch) => {
                    let result = self.fetch_blocking(&url).await;
                    trace.push(ToolUse::new(ToolKind::Fetch, &url, &result));
                    emit_tool(trace.last().expect("just pushed"));
                    let page = result.unwrap_or_else(|e| format!("{TOOL_ERROR}{e}"));
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&page);
                }
                ToolCall::Shell(shell_cmd) if tools.allows(ToolKind::Shell) => {
                    let result = self
                        .shell_blocking_on(Arc::clone(&runner), &shell_cmd)
                        .await;
                    let mut use_ = ToolUse::new(ToolKind::Shell, &shell_cmd, &result);
                    if result.is_ok() {
                        for path in
                            self.scan_artifacts(&runner, result.as_deref().unwrap_or_default())
                        {
                            use_ = use_.with_artifact(&path);
                        }
                    }
                    trace.push(use_);
                    emit_tool(trace.last().expect("just pushed"));
                    let out = result.unwrap_or_else(|e| format!("{TOOL_ERROR}{e}"));
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&out);
                }
                ToolCall::Coding { path, code } if tools.allows(ToolKind::Coding) => {
                    let bytes = code.len();
                    let label = format!("write {path} ({bytes} bytes)");
                    let label_path = path.clone();
                    let result =
                        tokio::task::spawn_blocking(move || runner.write_file(&path, &code))
                            .await
                            .map_err(|_| "coding task panicked".to_string())
                            .and_then(|inner| inner)
                            .map(|_| "file written".to_string());
                    let mut use_ = ToolUse::new(ToolKind::Coding, &label, &result);
                    if result.is_ok() {
                        use_ = use_.with_artifact(&label_path);
                    }
                    trace.push(use_);
                    emit_tool(trace.last().expect("just pushed"));
                    let out = result.unwrap_or_else(|e| format!("{TOOL_ERROR}{e}"));
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&out);
                }
                ToolCall::AgentRun { agent, cmd } if tools.allows(ToolKind::Agent) => {
                    let label = format!("run {agent}: {cmd}");
                    let result = match self.agent_run.as_ref() {
                        Some(run) => {
                            let run = Arc::clone(run);
                            tokio::task::spawn_blocking(move || run.run_for(&agent, &cmd))
                                .await
                                .map_err(|_| "agent run task panicked".to_string())
                                .and_then(|inner| inner)
                        }
                        None => Err("no agent runner is configured".to_string()),
                    };
                    trace.push(ToolUse::new(ToolKind::Agent, &label, &result));
                    emit_tool(trace.last().expect("just pushed"));
                    let out = result.unwrap_or_else(|e| format!("{TOOL_ERROR}{e}"));
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&out);
                }
                ToolCall::Math(expr) if tools.allows(ToolKind::Math) => {
                    let result = math::geomath::eval(&expr);
                    trace.push(ToolUse::new(ToolKind::Math, &expr, &result));
                    emit_tool(trace.last().expect("just pushed"));
                    let out = result.unwrap_or_else(|e| format!("{TOOL_ERROR}{e}"));
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&out);
                }
                ToolCall::Git { op, agent } if tools.allows(ToolKind::Git) => {
                    let agent = agent
                        .or_else(|| cmd.agent.clone())
                        .map(|a| a.trim().to_string())
                        .unwrap_or_default();
                    let (input, result) = if op.sandbox_only() && agent.is_empty() {
                        (
                            "git work".to_string(),
                            Err("branch/commit/push/pr run inside a named agent's own container — spawn an agent first".to_string()),
                        )
                    } else {
                        match Self::resolve_git_op(op, bound.as_ref()) {
                            Err(e) => ("clone".to_string(), Err(e)),
                            Ok((op, input)) => {
                                let result = match (self.agent_git.as_ref(), !agent.is_empty()) {
                                    (Some(git), true) => {
                                        let git = Arc::clone(git);
                                        tokio::task::spawn_blocking(move || {
                                            git.git_for(&agent, &op)
                                        })
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
                        }
                    };
                    trace.push(ToolUse::new(ToolKind::Git, &input, &result));
                    emit_tool(trace.last().expect("just pushed"));
                    let out = result.unwrap_or_else(|e| format!("{TOOL_ERROR}{e}"));
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&out);
                }
                ToolCall::Lsp {
                    op,
                    path,
                    line,
                    col,
                } if tools.allows(ToolKind::Lsp) => {
                    let input = format!("{} {path}:{line}:{col}", op.as_str().to_lowercase());
                    let result = self
                        .lsp_blocking(Arc::clone(&runner), op, &path, line, col)
                        .await;
                    trace.push(ToolUse::new(ToolKind::Lsp, &input, &result));
                    emit_tool(trace.last().expect("just pushed"));
                    let out = result.unwrap_or_else(|e| format!("{TOOL_ERROR}{e}"));
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&out);
                }
                call if Self::board_kind(&call).is_some_and(|k| tools.allows(k)) => {
                    let token = cmd.board_token.clone();
                    let kind = Self::board_kind(&call).expect("matched guard");
                    let op = Self::board_op(&call).expect("matched guard");
                    let out = self.board_blocking(token, op).await;
                    // Track card-flow progress: a created card with an agent
                    // but no run gets auto-run after the loop (see below).
                    match &call {
                        ToolCall::CardCreate { .. } => {
                            created_card = parse_card_id(&out);
                            card_ran = false;
                        }
                        ToolCall::CardAgent { .. } => card_assigned = true,
                        ToolCall::CardRun { .. } => {
                            card_ran = true;
                            created_card = None;
                        }
                        ToolCall::CardSchedule { .. } => card_scheduled = true,
                        _ => {}
                    }
                    let ok = !out.starts_with(BOARD_ERROR_PREFIX);
                    trace.push(ToolUse {
                        kind,
                        input: String::new(),
                        ok,
                        summary: out.chars().take(TOOL_SUMMARY_MAX).collect(),
                        artifacts: Vec::new(),
                    });
                    emit_tool(trace.last().expect("just pushed"));
                    context.push_str(TOOL_RESULT_HEADER);
                    context.push_str(&out);
                }
                _ => {
                    trace.push(ToolUse::denied(ToolKind::Search, ""));
                    emit_tool(trace.last().expect("just pushed"));
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
                .infer(&engine, prompt, None, cmd.max_tokens, cmd.tokenizer, think)
                .await
            {
                Ok(r) => {
                    last_good = (r.text.clone(), r.model.clone(), r.stats);
                    r
                }
                Err(e) => {
                    tracing::warn!("chat tool round {rounds} inference failed: {e}");
                    return Ok(self
                        .finish(&base_runner, &cmd, last_good, searched, trace, memories)
                        .await);
                }
            };
        }

        // Option-2 fallback: agent bound, no tool ran, plain answer — for a
        // work request open a card and run it instead of chat-back.
        if let Some(text) = self.auto_card(&cmd, allow_tools, rounds, &reply).await {
            last_good.0 = text.clone();
            reply.text = text;
        }

        // The model created + assigned a card but dropped CARD_RUN: run it
        // here so a work turn always ends in an executed card. Skipped when
        // the card is on a schedule (it will run when due).
        if let (Some(card_id), true, false, false) =
            (created_card, card_assigned, card_ran, card_scheduled)
        {
            let run = self
                .board_blocking(cmd.board_token.clone(), BoardOp::RunCard { card_id })
                .await;
            let note = format!("\n\n{AUTO_RUN_NOTE}{run}");
            last_good.0.push_str(&note);
            reply.text.push_str(&note);
        }

        // Tool budget exhausted while the model still wants a tool: force a
        // final answer on the gathered context, without the tool offer.
        if allow_tools && rounds > 0 && ToolCall::offers(&reply.text) {
            reply = match self
                .infer(
                    &engine,
                    Prompt::build(&cmd.message, &context, false, false, &tools),
                    None,
                    cmd.max_tokens,
                    cmd.tokenizer,
                    think,
                )
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("chat final answer inference failed: {e}");
                    return Ok(self
                        .finish(&base_runner, &cmd, last_good, searched, trace, memories)
                        .await);
                }
            };
        }

        if let Some(thread) = cmd.thread_id.as_deref() {
            self.remember_blocking(thread, "user", &cmd.message).await;
            self.remember_blocking(thread, "assistant", &reply.text)
                .await;
        }

        Ok(self
            .finish(
                &base_runner,
                &cmd,
                (reply.text, reply.model, reply.stats),
                searched,
                trace,
                memories,
            )
            .await)
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
