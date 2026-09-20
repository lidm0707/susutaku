//! Runs a card's assigned agent: the agent config (persona, instruction,
//! output format) is prepended to the card's title/description and submitted
//! to the model; the run ledger is merged into the card's agent state.
//!
//! Work-tree mode: when the card has an agent, a project with a bound git
//! repo and a [`WorkTree`] is wired in, the run executes against a real
//! manager task slot (model edits files via toolcalls), then publishes the
//! branch and opens a GitHub PR; the terminal output lands on the card as a
//! comment.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::domain::CancelFlag;

use manager_rs::manager::{Manager, RemoteRepo, TaskOutcome};
use task_rs::store::{CardRow, RunRecordNew, StoreError};
use task_rs::{AgentConfigRow, SkillRow, TaskStatus};

use crate::domain::{CardMove, GitOp, ToolCall};
use crate::infra::manager_git::ManagerGit;
use crate::infra::zai::settings::SettingsState;
use crate::port::outbound::{Inference, ModelEngines};
use latenspace::{JsonStuck, ToolPruner, check_json, repair_prompt};
use manager_rs::git_state::NO_CHANGES;

use super::task::TaskApp;

pub const RUN_KEY: &str = "run";
pub const RUN_PROGRESS_KEY: &str = "progress";
pub const MOVE_POSITION_TOP: i32 = 0;
pub const NOTE_NO_AGENT: &str = "no agent assigned to the card";
pub const NOTE_RUN_CANCELLED: &str = "run cancelled by user";
pub const NOTE_NO_ENGINE: &str = "no inference engine configured";
pub const INFER_MAX_TOKENS: usize = 1024;
pub const OUTPUT_PREVIEW_MAX: usize = 400;
pub const PROMPT_PERSONA: &str = "persona: ";
pub const PROMPT_INSTRUCTION: &str = "instruction: ";
pub const PROMPT_OUTPUT: &str = "output format: ";
pub const PROMPT_TASK: &str = "\n\ntask:\n";
pub const PROMPT_SKILLS: &str = "\n\nproject skills:\n";
pub const TEXT_SEP: &str = "\n\n";
/// Per-skill body cap so a long skill sheet cannot eat the prompt budget.
pub const MAX_SKILL_CHARS: usize = 6000;
/// Coding runs need far more turns than chat (clone, explore, edit several
/// files, verify, commit) — chat's TOOL_ROUNDS_MAX starves them.
pub const CARD_TOOL_ROUNDS: usize = 24;

/// Fed back to the model when it offers a tool that does not parse, so the
/// run retries with a well-formed call instead of dead-ending.
pub const CARD_TOOL_MALFORMED: &str = "malformed tool call: nothing was run. Every tool line MUST start with exactly 'TOOL: ' at the start of a line, e.g. `TOOL: SHELL cargo check`, or use a full <invoke name=\"...\"> block with every required <parameter name=\"...\">. Repeat the call well-formed.";

/// What the work loop does with one model reply: run the parsed tool,
/// retry once on a malformed tool offer, or take the text as the final
/// answer.
pub enum WorkStep {
    Tool(ToolCall),
    Retry,
    Final(String),
}

pub fn work_step(reply: String) -> WorkStep {
    match ToolCall::parse(&reply) {
        Some(call) => WorkStep::Tool(call),
        None if ToolCall::offers(&reply) => WorkStep::Retry,
        None => WorkStep::Final(reply),
    }
}
pub const WORK_MAX_TOKENS: usize = 2048;
pub const TOOL_OUTPUT_MAX: usize = 2000;
pub const OUTPUT_COMMENT_MAX: usize = 8000;
pub const SLOT_KEY_SEP: char = manager_rs::manager::TASK_KEY_SEP;
pub const NOTE_SPAWN_ERR: &str = "work tree spawn failed: ";
pub const NOTE_PUBLISH_ERR: &str = "publish failed: ";
pub const NOTE_WRITE_ERR: &str = "file write failed: ";
pub const NOTE_JOIN: &str = "manager task panicked";
pub const DENIED_NOTE: &str =
    "tool denied in a card run: use SHELL, AGENT_RUN, GIT or the coding write_file block";
pub const NOTE_BRANCH_DENIED: &str = "branch switch denied in a card run: all work stays on the checked-out task branch — the run pushes exactly that branch and opens the PR";
/// `{branch}` placeholder: names the run's task branch so the model never
/// invents its own (e.g. `test-push-branch`) and never commits to main.
pub const BRANCH_RULE: &str = "BRANCH: the task branch `{branch}` is already checked out. Do ALL work on it: never create or switch branches (GIT BRANCH is denied), never commit to main. The run commits, pushes exactly `{branch}` and opens the PR `{pr_base}` <- `{branch}`.\n";
pub const TOOL_HINT: &str = "You are working alone in a sandboxed work tree of the project's git repo (a task branch is checked out). Edit real files, one tool call per reply. Every tool line MUST start with exactly 'TOOL: ' — a line that only says 'SHELL ...' is NOT a tool call and ends the run:\n- TOOL: SHELL <cmd> - run a shell command in the work tree\n- TOOL: AGENT_RUN <cmd> - run a shell command in the work tree (alias)\n- TOOL: GIT STATUS | DIFF | BRANCH <name> | COMMIT <message> | PUSH <branch> | PR <title>\n- coding block: <invoke name=\"coding\"><parameter name=\"path\">rel/path</parameter><parameter name=\"code\">file content</parameter></invoke>\nBUDGET: at most 2 exploration rounds (ls/cat/grep). After that you MUST write code — use the coding block to create or edit at least one file EVERY round until the feature is complete. Reading is NOT progress. The run is graded on files changed. When done, reply with a final text summary (no tool line); the run publishes the branch and opens a PR automatically.\n";
pub const PUBLISH_BRANCH: &str = "\n\nbranch: ";
pub const PUBLISH_PUSH: &str = "\npush: ";
pub const PUBLISH_PR: &str = "\npr: ";
pub const PUBLISH_PR_FAILED: &str = "\npr: failed: ";
pub const NOTE_NOTHING_PUBLISHED: &str = "run produced no commits — nothing to publish; the agent made no file changes, refine the task description or re-run";
pub const NOTE_EVIDENCE: &str = "\n\nlast reply:\n";
pub const NOTE_TRANSCRIPT: &str = "\n\nlast steps:\n";
pub const TRANSCRIPT_TAIL_MAX: usize = 2000;
/// Token budget for the work-loop transcript: the kat pruner evicts cold,
/// old tool outputs first instead of letting round 24 carry rounds 1..24.
pub const TRANSCRIPT_BUDGET_TOKENS: usize = latenspace::kat_tool_call::PRUNER_DEFAULT_BUDGET;
pub const NOTE_ROUNDS_OUT: &str = "\n\nrun ended: out of tool rounds before the agent finished";

/// `run_events.kind` values: the full agent stream (generation, tool calls,
/// command output, retries) persisted per card.
pub const RUN_EVENT_RUN: &str = "run";
pub const RUN_EVENT_GEN: &str = "gen";
pub const RUN_EVENT_TOOL: &str = "tool";
pub const RUN_EVENT_OUT: &str = "out";
pub const RUN_EVENT_RETRY: &str = "retry";
pub const RUN_EVENT_NOTE: &str = "note";
pub const RUN_EVENT_FINAL: &str = "final";

/// Retry-with-context: a fresh run primed with the prior run's event stream.
pub const CONTEXT_HEADING: &str = "PREVIOUS ATTEMPT CONTEXT: a prior run of this card produced the events below (its final failure included). Use them as context and continue or complete the task.\n";
pub const CONTEXT_LABEL: &str = "previous run";
pub const CONTEXT_EVENT_MAX: usize = 1500;
pub const RETRY_CONTEXT_EVENTS: i64 = 40;

/// Append one full-text run event and publish the board event so open cards
/// stream it live over the websocket.
async fn log_event(app: &TaskApp, card_id: i64, kind: &str, text: &str) {
    if text.is_empty() {
        return;
    }
    if app
        .store
        .insert_run_event(card_id, kind, text)
        .await
        .is_ok()
    {
        crate::app::events::publish(crate::app::events::EventKind::Card);
    }
}
/// Bare verb lines accepted as tool calls when the model skipped the exact
/// `TOOL: ` prefix (work runs only; chat keeps strict parsing).
pub const LENIENT_TOOLS: [&str; 4] = ["SHELL", "AGENT_RUN", "GIT", "LSP"];
pub const LENIENT_PREFIX: &str = "TOOL: ";
pub const REMINDER_LABEL: &str = "assistant";
pub const MALFORMED_LABEL: &str = "malformed";
/// No-tool replies tolerated before the run ends: one strict reminder per
/// reply. Small models often need a second nudge before they emit a tool
/// line, and each extra nudge is cheaper than a wasted failed run.
pub const REMINDERS_MAX: usize = 2;
pub const WORK_REMINDER: &str = "Your reply contained no tool call and no code, so nothing was done. You MUST reply with a line starting exactly `TOOL: ` (e.g. `TOOL: SHELL <cmd>`), or a coding block, to act on the work tree. Plain explanations are NOT accepted before the work is done.";
pub const PR_BASE: &str = "main";
pub const TASK_BRANCH_PREFIX: &str = "task/";
pub const BRANCH_SEP: char = '-';
pub const COMMIT_PREFIX: &str = "agent task: ";
pub const PATH_ESCAPE: &str = "path escape rejected: ";

/// Optional work-tree dependency: wraps the manager so card runs execute in
/// a real sandboxed work tree instead of pure text inference.
pub struct WorkTree {
    manager: Arc<Manager>,
}

impl WorkTree {
    pub fn new(manager: Arc<Manager>) -> Self {
        Self { manager }
    }

    pub async fn spawn_task(
        &self,
        agent: &str,
        slot: &str,
        task: &str,
        repo: RemoteRepo,
        image: Option<String>,
    ) -> Result<PathBuf, String> {
        let (m, agent, slot, task) = (
            self.manager.clone(),
            agent.to_owned(),
            slot.to_owned(),
            task.to_owned(),
        );
        tokio::task::spawn_blocking(move || {
            // A card run must start from the bound repo's HEAD with a task
            // branch; a reused stale tree (seeded before the repo was bound,
            // or after a failed clone) has no branch and can never publish.
            if m.snapshot().iter().any(|a| a.agent == slot) {
                let _ = m.finish(&slot);
            }
            // The manager composes the slot key as agent#task itself.
            m.spawn_task_with_repo(&agent, &task, &repo, image.as_deref())
        })
        .await
        .map_err(|_| NOTE_JOIN.to_owned())?
    }

    pub async fn run(&self, slot: &str, cmd: &str) -> Result<String, String> {
        let (m, slot, cmd) = (self.manager.clone(), slot.to_owned(), cmd.to_owned());
        tokio::task::spawn_blocking(move || m.run_with_network(&slot, &cmd))
            .await
            .map_err(|_| NOTE_JOIN)?
    }

    pub async fn git(&self, slot: &str, tool: proto_rs::GitTool) -> Result<String, String> {
        let (m, slot) = (self.manager.clone(), slot.to_owned());
        tokio::task::spawn_blocking(move || m.git_tool(&slot, &tool))
            .await
            .map_err(|_| NOTE_JOIN.to_owned())?
    }

    pub async fn task_has_commits(&self, slot: &str) -> Result<bool, String> {
        let (m, slot) = (self.manager.clone(), slot.to_owned());
        tokio::task::spawn_blocking(move || m.task_has_commits(&slot))
            .await
            .map_err(|_| NOTE_JOIN.to_owned())?
    }

    /// The slot's task branch as recorded at spawn — the single source of
    /// truth for publish; falls back to the derived name for legacy slots.
    pub async fn slot_branch(&self, slot: &str) -> Result<Option<String>, String> {
        let (m, slot) = (self.manager.clone(), slot.to_owned());
        tokio::task::spawn_blocking(move || m.slot_task_branch(&slot))
            .await
            .map_err(|_| NOTE_JOIN.to_owned())?
    }

    pub async fn finish(&self, slot: &str) -> Result<TaskOutcome, String> {
        let (m, slot) = (self.manager.clone(), slot.to_owned());
        tokio::task::spawn_blocking(move || m.finish(&slot))
            .await
            .map_err(|_| NOTE_JOIN.to_owned())?
    }
}

/// Repo a work-tree run is bound to, plus the manager slot identity:
/// slot key `agent#<card_id>`, task string `<card_id>` (branch `task/<id>-<agent>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundRepo {
    pub slot: String,
    pub agent: String,
    pub task: String,
    pub url: String,
    pub token: String,
    /// Card-pinned sandbox image; None = agent default.
    pub image: Option<String>,
}

enum RunMode {
    Text,
    Work(BoundRepo),
}

/// Work-tree mode iff a work tree is wired, the card has an agent and a
/// project with a bound git repo; otherwise legacy text inference.
fn select_mode(wt: Option<&WorkTree>, card: &CardRow) -> RunMode {
    let (Some(_), Some(agent), Some(project_id)) =
        (wt, card.agent_name.as_deref(), card.project_id)
    else {
        return RunMode::Text;
    };
    match SettingsState::load().git_repo(project_id) {
        Some(repo) => RunMode::Work(BoundRepo {
            slot: slot_key(agent, card.id),
            agent: agent.to_owned(),
            task: card.id.to_string(),
            url: repo.url,
            token: repo.secret.unwrap_or_default(),
            image: card.image.clone(),
        }),
        None => RunMode::Text,
    }
}

/// Manager slot key: one container + one task branch per (agent, card).
pub fn slot_key(agent: &str, card_id: i64) -> String {
    format!("{agent}{SLOT_KEY_SEP}{card_id}")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Ok,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct RunRecord {
    pub agent: String,
    pub status: RunStatus,
    pub output: Option<String>,
    pub finished_at: String,
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Registry of in-flight card runs: card id → cancel flag.
pub type CardCancels = Arc<RwLock<HashMap<i64, CancelFlag>>>;

static CARD_CANCELS: std::sync::OnceLock<CardCancels> = std::sync::OnceLock::new();

fn cancel_registry() -> CardCancels {
    CARD_CANCELS
        .get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
        .clone()
}

/// Flag the in-flight run for `card_id` (if any). Returns true when a run
/// was live; the run notices at the next tool round and tears down.
pub fn cancel_run(card_id: i64) -> bool {
    let registry = cancel_registry();
    let flag = registry.read().ok().and_then(|r| r.get(&card_id).cloned());
    let ok = flag.is_some_and(|f| {
        f.cancel();
        true
    });
    if ok {
        crate::app::events::publish(crate::app::events::EventKind::Card);
    }
    ok
}

fn register_cancel(card_id: i64) -> CancelFlag {
    let cancel = CancelFlag::new();
    if let Ok(mut r) = cancel_registry().write() {
        r.insert(card_id, cancel.clone());
    }
    cancel
}

fn unregister_cancel(card_id: i64) {
    if let Ok(mut r) = cancel_registry().write() {
        r.remove(&card_id);
    }
}

/// Per-run controls threaded through execution: the cancel flag and the
/// optional prior-attempt context a retry run is primed with.
struct RunCtx<'a> {
    cancel: &'a CancelFlag,
    context: Option<&'a str>,
}

/// Run the card's assigned agent once. A card without an agent still records
/// a failed run so the card moves on instead of silently staying put.
/// The agent's configured model is routed through `engines` first (cloud
/// models); models it does not know fall back to the shared `engine`.
pub async fn run_card(
    app: &TaskApp,
    engine: Option<Arc<dyn Inference>>,
    engines: Option<&dyn ModelEngines>,
    card_id: i64,
    trigger: &'static str,
    wt: Option<&WorkTree>,
    context: Option<&str>,
) -> Result<RunRecord, StoreError> {
    let card = app
        .cards
        .get(card_id)
        .await?
        .ok_or(StoreError::NoSuchCard)?;
    let agent = card.agent_name.clone().unwrap_or_default();
    if app.cards.set_run_start(card_id, &agent).await.is_ok() {
        crate::app::events::publish(crate::app::events::EventKind::Card);
        log_event(app, card_id, RUN_EVENT_RUN, &agent).await;
    }
    move_to_column(app, card_id, &card.column_id, TaskStatus::InProgress).await?;
    let cancel = register_cancel(card_id);
    let ctx = RunCtx {
        cancel: &cancel,
        context,
    };
    let record = match select_mode(wt, &card) {
        RunMode::Text => execute(app, engine.as_ref(), engines, &card, &ctx).await,
        RunMode::Work(bound) => {
            execute_work(
                app,
                engine.as_ref(),
                engines,
                &card,
                wt.expect("mode selected with work tree"),
                bound,
                &ctx,
            )
            .await
        }
    };
    unregister_cancel(card_id);
    let record = persist(app, card_id, &card, record, trigger).await?;
    log_event(
        app,
        card_id,
        RUN_EVENT_FINAL,
        &record.output.clone().unwrap_or_default(),
    )
    .await;
    Ok(record)
}

async fn execute(
    app: &TaskApp,
    engine: Option<&Arc<dyn Inference>>,
    engines: Option<&dyn ModelEngines>,
    card: &CardRow,
    ctx: &RunCtx<'_>,
) -> RunRecord {
    let agent = card.agent_name.clone().unwrap_or_default();
    match (engine, card.agent_name.as_deref()) {
        (_, None) => fail(&agent, NOTE_NO_AGENT),
        (None, Some(_)) => fail(&agent, NOTE_NO_ENGINE),
        (Some(engine), Some(name)) => match app.agents.by_name(name).await.ok().flatten() {
            None => fail(&agent, NOTE_NO_AGENT),
            Some(cfg) => {
                if ctx.cancel.is_cancelled() {
                    return fail(&agent, NOTE_RUN_CANCELLED);
                }
                let model = cfg.model.trim();
                let routed = engines
                    .and_then(|e| e.engine_for(model))
                    .filter(|_| !model.is_empty());
                let target = routed.as_ref().unwrap_or(engine);
                let skills = agent_skills(app, &cfg).await;
                infer(app, target, &cfg, card, &agent, &skills, ctx.context).await
            }
        },
    }
}

fn fail(agent: &str, note: &str) -> RunRecord {
    RunRecord {
        agent: agent.to_owned(),
        status: RunStatus::Failed,
        output: Some(note.to_owned()),
        finished_at: now_iso(),
    }
}

async fn infer(
    app: &TaskApp,
    engine: &Arc<dyn Inference>,
    cfg: &AgentConfigRow,
    card: &CardRow,
    agent: &str,
    skills: &[SkillRow],
    context: Option<&str>,
) -> RunRecord {
    let prompt = with_context(task_prompt(cfg, card, skills), context);
    match submit_retry(app, card.id, engine, None, cfg, prompt, INFER_MAX_TOKENS).await {
        Ok(text) => RunRecord {
            agent: agent.to_owned(),
            status: RunStatus::Ok,
            output: Some(text),
            finished_at: now_iso(),
        },
        Err(e) => fail(agent, &format!("{NOTE_INFER_ERR}{e}")),
    }
}

fn prompt_head(cfg: &AgentConfigRow, skills: &[SkillRow]) -> String {
    let mut prompt = String::new();
    for (header, body) in [
        (PROMPT_PERSONA, cfg.persona.as_str()),
        (PROMPT_INSTRUCTION, cfg.prompt.as_str()),
        (PROMPT_OUTPUT, cfg.output.as_str()),
    ] {
        if !body.is_empty() {
            prompt.push_str(header);
            prompt.push_str(body);
            prompt.push('\n');
        }
    }
    if !skills.is_empty() {
        prompt.push_str(PROMPT_SKILLS);
        for skill in skills {
            prompt.push_str("## ");
            prompt.push_str(&skill.name);
            prompt.push('\n');
            let body = skill.body.trim();
            let end = body
                .char_indices()
                .nth(MAX_SKILL_CHARS)
                .map(|(i, _)| i)
                .unwrap_or(body.len());
            prompt.push_str(body.get(..end).unwrap_or(body));
            prompt.push('\n');
        }
    }
    prompt
}

fn task_body(card: &CardRow) -> String {
    let mut task = card.title.clone();
    if !card.description.is_empty() {
        task.push_str(TEXT_SEP);
        task.push_str(&card.description);
    }
    task
}

pub fn task_prompt(cfg: &AgentConfigRow, card: &CardRow, skills: &[SkillRow]) -> String {
    let mut prompt = prompt_head(cfg, skills);
    prompt.push_str(PROMPT_TASK);
    prompt.push_str(&task_body(card));
    prompt
}

/// Append the prior-attempt context block to a base prompt.
fn with_context(base: String, context: Option<&str>) -> String {
    match context {
        Some(c) if !c.trim().is_empty() => {
            let mut prompt = base;
            prompt.push_str(TEXT_SEP);
            prompt.push_str(CONTEXT_HEADING);
            prompt.push_str(c);
            prompt
        }
        _ => base,
    }
}

/// Tail of the card's prior run events, trimmed per event — the context a
/// "retry" run is primed with.
pub async fn prior_context(app: &TaskApp, card_id: i64) -> Option<String> {
    let rows = app
        .store
        .list_run_events(card_id, 0, RETRY_CONTEXT_EVENTS)
        .await
        .ok()?;
    let mut out = String::new();
    for row in rows {
        let text: String = row.text.chars().take(CONTEXT_EVENT_MAX).collect();
        if text.trim().is_empty() {
            continue;
        }
        out.push('[');
        out.push_str(&row.kind);
        out.push_str("] ");
        out.push_str(&text);
        out.push('\n');
    }
    (!out.is_empty()).then_some(out)
}

pub fn work_prompt(
    cfg: &AgentConfigRow,
    card: &CardRow,
    transcript: &str,
    skills: &[SkillRow],
    branch: &str,
) -> String {
    let mut prompt = task_prompt(cfg, card, skills);
    prompt.push_str(TEXT_SEP);
    prompt.push_str(
        &BRANCH_RULE
            .replace("{branch}", branch)
            .replace("{pr_base}", PR_BASE),
    );
    prompt.push_str(TOOL_HINT);
    prompt.push_str(transcript);
    prompt
}

/// Attached skill sheets (the seeded `susutaku-project` skill among them) —
/// without these the worktree agent has no idea how this project builds,
/// tests or where its rules live.
async fn agent_skills(app: &TaskApp, cfg: &AgentConfigRow) -> Vec<SkillRow> {
    app.store
        .list_agent_skills(cfg.id)
        .await
        .unwrap_or_default()
}

/// One inference turn routed through the per-model cloud engine when known.
async fn submit(
    engine: &Arc<dyn Inference>,
    engines: Option<&dyn ModelEngines>,
    cfg: &AgentConfigRow,
    prompt: String,
    max_tokens: usize,
) -> Result<String, String> {
    use susutaku_mlx::tok::TokKind;
    let model = cfg.model.trim();
    let routed = engines
        .and_then(|e| e.engine_for(model))
        .filter(|_| !model.is_empty());
    let target = routed.as_ref().unwrap_or(engine);
    let rx = target
        .submit(prompt, max_tokens, TokKind::Normal, false)
        .map_err(|e| e.to_string())?;
    match tokio::time::timeout(std::time::Duration::from_secs(INFER_TURN_TIMEOUT_SECS), rx).await {
        Ok(Ok(Ok(reply))) => Ok(reply.text),
        // Surface the engine's own error — collapsing it into the generic
        // drop note hides the root cause (http failure, bad reply, …).
        Ok(Ok(Err(e))) => Err(e),
        Ok(Err(_)) | Err(_) => Err(NOTE_INFER_DROP.to_owned()),
    }
}

pub const NOTE_INFER_ERR: &str = "inference failed: ";
pub const NOTE_INFER_DROP: &str = "inference: engine dropped or rejected the job";
pub const INFER_RETRY_MAX: usize = 5;
pub const INFER_RETRY_DELAY_MS: u64 = 2000;
pub const MS_PER_S: u64 = 1000;
/// Hard bound on one inference turn: a stalled cloud stream (SSE reader
/// blocked with no data) must fail the turn and trigger a retry instead of
/// parking the run in "running" forever.
pub const INFER_TURN_TIMEOUT_SECS: u64 = 600;
pub const PROGRESS_TOOL_INFER: &str = "inference";
pub const RETRY_LABEL: &str = "retry";
pub const RETRY_NEXT_IN: &str = "next in";
pub const RETRY_ATTEMPTS_NOTE: &str = "attempts";

/// One inference turn with bounded retries. Every failed attempt is written
/// to the card progress so the UI can show the retry count and next retry.
async fn submit_retry(
    app: &TaskApp,
    card_id: i64,
    engine: &Arc<dyn Inference>,
    engines: Option<&dyn ModelEngines>,
    cfg: &AgentConfigRow,
    prompt: String,
    max_tokens: usize,
) -> Result<String, String> {
    for attempt in 1..=INFER_RETRY_MAX {
        match submit(engine, engines, cfg, prompt.clone(), max_tokens).await {
            Ok(text) => {
                log_event(app, card_id, RUN_EVENT_GEN, &text).await;
                return Ok(text);
            }
            Err(e) if attempt < INFER_RETRY_MAX => {
                tracing::warn!(attempt, error = %e, "inference retry scheduled");
                report_retry(app, card_id, attempt).await;
                tokio::time::sleep(std::time::Duration::from_millis(INFER_RETRY_DELAY_MS)).await;
            }
            Err(e) => return Err(format!("{e} after {INFER_RETRY_MAX} {RETRY_ATTEMPTS_NOTE}")),
        }
    }
    unreachable!("retry loop always returns")
}

async fn report_retry(app: &TaskApp, card_id: i64, failed_attempt: usize) {
    let delay_s = INFER_RETRY_DELAY_MS / MS_PER_S;
    let progress = serde_json::json!({
        "round": failed_attempt + 1,
        "rounds": INFER_RETRY_MAX,
        "last_tool": PROGRESS_TOOL_INFER,
        "last_output": format!(
            "{RETRY_LABEL} {}/{} — {RETRY_NEXT_IN} {delay_s}s",
            failed_attempt + 1,
            INFER_RETRY_MAX
        ),
        "retry": true,
        "updated_at": now_iso(),
    });
    write_progress(app, card_id, progress).await;
    log_event(
        app,
        card_id,
        RUN_EVENT_RETRY,
        &format!(
            "{RETRY_LABEL} {}/{} — {RETRY_NEXT_IN} {delay_s}s",
            failed_attempt + 1,
            INFER_RETRY_MAX
        ),
    )
    .await;
}

/// Move a task unless it already sits in the target column.
async fn move_to_column(
    app: &TaskApp,
    card_id: i64,
    current: &str,
    target: TaskStatus,
) -> Result<(), StoreError> {
    if current == target.column() {
        return Ok(());
    }
    app.cards
        .move_card(CardMove {
            id: card_id,
            column_id: target.column().to_owned(),
            position: MOVE_POSITION_TOP,
        })
        .await
}

async fn persist(
    app: &TaskApp,
    card_id: i64,
    card: &CardRow,
    record: RunRecord,
    trigger: &'static str,
) -> Result<RunRecord, StoreError> {
    // A null/array/scalar agent_state (e.g. a set_agent write with state:
    // null) is as good as no state: only an object can carry the run ledger.
    let mut state = card
        .agent_state
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    // The live-progress snapshot is superseded by the finished run record.
    state.remove(RUN_PROGRESS_KEY);
    state.insert(
        RUN_KEY.to_owned(),
        serde_json::to_value(&record).map_err(|e| StoreError::BadSpec(e.to_string()))?,
    );
    // Preference (`agent_name`) is never mutated by a run: the state write
    // touches only the ledger.
    app.cards
        .set_agent_state(
            card_id,
            &serde_json::to_string(&state).map_err(|e| StoreError::BadSpec(e.to_string()))?,
        )
        .await?;
    let ok = record.status == RunStatus::Ok;
    let summary = record.output.clone().unwrap_or_default();
    let summary: String = summary.chars().take(OUTPUT_PREVIEW_MAX).collect();
    app.cards
        .record_run(RunRecordNew {
            card_id,
            trigger: trigger.to_owned(),
            agent: record.agent.clone(),
            ok,
            summary,
        })
        .await?;
    let target = match record.status {
        RunStatus::Ok => TaskStatus::Done,
        RunStatus::Failed => TaskStatus::Failed,
    };
    move_to_column(app, card_id, &card.column_id, target).await?;
    crate::app::events::publish(crate::app::events::EventKind::Card);
    Ok(record)
}

async fn execute_work(
    app: &TaskApp,
    engine: Option<&Arc<dyn Inference>>,
    engines: Option<&dyn ModelEngines>,
    card: &CardRow,
    wt: &WorkTree,
    bound: BoundRepo,
    ctx: &RunCtx<'_>,
) -> RunRecord {
    let agent = card.agent_name.clone().unwrap_or_default();
    let Some(engine) = engine else {
        return fail(&agent, NOTE_NO_ENGINE);
    };
    let Some(cfg) = app.agents.by_name(&agent).await.ok().flatten() else {
        return fail(&agent, NOTE_NO_AGENT);
    };
    let skills = agent_skills(app, &cfg).await;
    let repo = RemoteRepo {
        url: bound.url.clone(),
        token: (!bound.token.is_empty()).then(|| bound.token.clone()),
    };
    let work_tree = match wt
        .spawn_task(
            &bound.agent,
            &bound.slot,
            &bound.task,
            repo,
            bound.image.clone(),
        )
        .await
    {
        Ok(tree) => tree,
        Err(e) => return fail(&agent, &format!("{NOTE_SPAWN_ERR}{e}")),
    };
    // One branch name for the whole run: prompt, push and PR all use the
    // slot's spawn-time task branch (fallback derives the same name).
    let branch = match wt.slot_branch(&bound.slot).await {
        Ok(Some(b)) => b,
        Ok(None) => task_branch(&bound.agent, &bound.task),
        Err(e) => return fail(&agent, &format!("{NOTE_SPAWN_ERR}{e}")),
    };
    let mut transcript = ToolPruner::with_counter(
        ToolPruner::new(TRANSCRIPT_BUDGET_TOKENS),
        Box::new(modelless::policy::token_count),
    );
    let mut final_text = String::new();
    let mut no_tool_replies = 0usize;
    let mut ran_out = true;
    if let Some(c) = ctx.context.filter(|c| !c.trim().is_empty()) {
        append_tool(&mut transcript, CONTEXT_LABEL, c);
    }
    for round in 0..CARD_TOOL_ROUNDS {
        if ctx.cancel.is_cancelled() {
            ran_out = false;
            break;
        }
        let prompt = work_prompt(&cfg, card, &transcript.render(), &skills, &branch);
        let reply = match submit_retry(app, card.id, engine, engines, &cfg, prompt, WORK_MAX_TOKENS)
            .await
        {
            Ok(text) => text,
            Err(e) => return fail(&agent, &format!("{NOTE_INFER_ERR}{e}")),
        };
        let step = match ToolCall::parse(&reply).or_else(|| lenient_tool(&reply)) {
            Some(call) => WorkStep::Tool(call),
            // A malformed tool offer gets one corrective turn; a plain
            // prose reply gets a strict reminder (REMINDERS_MAX times);
            // then the reply becomes the final text and the run ends.
            None if ToolCall::offers(&reply) => WorkStep::Retry,
            None if no_tool_replies < REMINDERS_MAX => {
                no_tool_replies += 1;
                append_tool(&mut transcript, REMINDER_LABEL, &reply);
                append_tool(&mut transcript, REMINDER_LABEL, WORK_REMINDER);
                continue;
            }
            None => {
                final_text = reply;
                ran_out = false;
                break;
            }
        };
        match step {
            WorkStep::Tool(call) => {
                let label = call_label(&call);
                let out = exec_tool(wt, &bound, &work_tree, call).await;
                append_tool(&mut transcript, &label, &out);
                log_event(app, card.id, RUN_EVENT_TOOL, &label).await;
                log_event(app, card.id, RUN_EVENT_OUT, &out).await;
                report_progress(app, card.id, round, &label, &out).await;
            }
            WorkStep::Retry => {
                let note = retry_note(&reply);
                append_tool(&mut transcript, MALFORMED_LABEL, &note);
                log_event(app, card.id, RUN_EVENT_NOTE, &note).await;
            }
            WorkStep::Final(_) => unreachable!("final handled during parse"),
        }
    }
    let cancelled = ctx.cancel.is_cancelled();
    let rounds_note = if cancelled {
        NOTE_RUN_CANCELLED.to_owned()
    } else if ran_out {
        NOTE_ROUNDS_OUT.to_owned()
    } else {
        String::new()
    };
    let publish = if cancelled {
        Publish::Nothing
    } else {
        publish_work(wt, &bound, &branch, &card.title).await
    };
    teardown(app, card.id, wt, &bound.slot, &agent).await;
    match publish {
        // No commits is a failed run, not a passed one: the card moves to the
        // failed column so the board never shows a done card without a PR.
        // The evidence (last reply + tool trace tail) rides along in the
        // output so the run log shows WHY nothing changed.
        Publish::Nothing => fail(
            &agent,
            &format!(
                "{NOTE_NOTHING_PUBLISHED}{rounds_note}{}{}{NOTE_TRANSCRIPT}{}",
                NOTE_EVIDENCE,
                truncate(&final_text, TRANSCRIPT_TAIL_MAX),
                tail(&transcript.render(), TRANSCRIPT_TAIL_MAX)
            ),
        ),
        Publish::Failed(e) => fail(&agent, &format!("{NOTE_PUBLISH_ERR}{e}")),
        Publish::Done(branch, push, pr) => RunRecord {
            agent,
            status: RunStatus::Ok,
            output: Some(format!(
                "{final_text}{}{rounds_note}",
                publish_note(&branch, &push, &pr)
            )),
            finished_at: now_iso(),
        },
    }
}

/// Outcome of publishing a run's work.
enum Publish {
    /// No file changes — nothing to commit, push or open a PR for.
    Nothing,
    /// Branch name, push output, PR output (or the PR failure reason).
    Done(String, String, String),
    /// A hard failure (diff/commit/push step) — the run failed.
    Failed(String),
}

/// Commit the pending work, push the task branch, open the PR against
/// [`PR_BASE`]. Ships both committed AND uncommitted work: dirty edits are
/// committed here so an agent that edited but never ran GIT COMMIT still
/// lands a PR. The branch is [`WorkTree::slot_branch`] — recorded at spawn —
/// and is first reconciled to HEAD, so commits the agent stranded on another
/// branch (e.g. a self-invented `test-push-branch`) land on the task branch
/// before push and PR. A PR failure does not fail the run: the branch is
/// already on the remote and review can happen there.
async fn publish_work(wt: &WorkTree, bound: &BoundRepo, branch: &str, title: &str) -> Publish {
    let diff = match ManagerGit::to_proto(&GitOp::Diff) {
        Ok(tool) => wt.git(&bound.slot, tool).await,
        Err(e) => return Publish::Failed(e),
    };
    let dirty = match diff {
        Ok(d) => diff_is_dirty(&d),
        Err(e) => return Publish::Failed(e),
    };
    if dirty {
        // The commit script is a no-op when nothing is staged, so this is
        // safe even on a racy tree.
        let commit = ManagerGit::to_proto(&GitOp::Commit {
            message: format!("{COMMIT_PREFIX}{title}"),
        });
        if let Err(e) = match commit {
            Ok(tool) => wt.git(&bound.slot, tool).await,
            Err(e) => return Publish::Failed(e),
        } {
            return Publish::Failed(format!("commit: {e}"));
        }
    }
    // Gate AFTER the commit: push needs at least one commit past the spawn
    // HEAD, else the remote branch would point at main and every PR would
    // die with "No commits between main and <branch>".
    let has = match wt.task_has_commits(&bound.slot).await {
        Ok(v) => v,
        Err(e) => return Publish::Failed(e),
    };
    if !has {
        return Publish::Nothing;
    }
    let reconcile = proto_rs::GitTool::TaskBranch {
        name: branch.to_owned(),
    };
    if let Err(e) = wt.git(&bound.slot, reconcile).await {
        return Publish::Failed(format!("task branch: {e}"));
    }
    let push = ManagerGit::to_proto(&GitOp::Push {
        branch: branch.to_owned(),
        url: Some(bound.url.clone()),
        token: (!bound.token.is_empty()).then(|| bound.token.clone()),
    });
    let push_out = match push {
        Ok(tool) => wt.git(&bound.slot, tool).await,
        Err(e) => return Publish::Failed(format!("push: {e}")),
    };
    let push = match push_out {
        Ok(out) => out,
        Err(e) => return Publish::Failed(format!("push: {e}")),
    };
    let pr = match ManagerGit::to_proto(&GitOp::PullRequest {
        title: branch.to_owned(),
        head: branch.to_owned(),
        base: PR_BASE.to_owned(),
        url: Some(bound.url.clone()),
        token: (!bound.token.is_empty()).then(|| bound.token.clone()),
    }) {
        Ok(tool) => wt.git(&bound.slot, tool).await,
        Err(e) => return Publish::Done(branch.to_owned(), push, format!("failed: {e}")),
    };
    match pr {
        Ok(out) => Publish::Done(branch.to_owned(), push, out),
        Err(e) => Publish::Done(branch.to_owned(), push, format!("failed: {e}")),
    }
}

/// The manager's task branch name: task/<task>-<agent>, sanitized.
pub fn task_branch(agent: &str, task: &str) -> String {
    let clean = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    };
    format!(
        "{TASK_BRANCH_PREFIX}{}{BRANCH_SEP}{}",
        clean(task),
        clean(agent)
    )
}

fn publish_note(branch: &str, push: &str, pr: &str) -> String {
    format!(
        "{PUBLISH_BRANCH}{branch}{PUBLISH_PUSH}{push}{PUBLISH_PR}{}",
        pr.trim()
    )
}

/// Snapshot run progress into the card state so the UI can show what the
/// agent is doing mid-run (the board refreshes via the published event).
async fn report_progress(
    app: &TaskApp,
    card_id: i64,
    round: usize,
    last_tool: &str,
    last_output: &str,
) {
    let progress = serde_json::json!({
        "round": round + 1,
        "rounds": CARD_TOOL_ROUNDS,
        "last_tool": last_tool,
        "last_output": truncate(last_output, OUTPUT_PREVIEW_MAX),
        "updated_at": now_iso(),
    });
    write_progress(app, card_id, progress).await;
}

/// Persist a progress snapshot into the card state and publish the event.
async fn write_progress(app: &TaskApp, card_id: i64, progress: serde_json::Value) {
    let Ok(Some(card)) = app.cards.get(card_id).await else {
        return;
    };
    let mut state = card
        .agent_state
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    state.insert(RUN_PROGRESS_KEY.to_owned(), progress);
    let Ok(state) = serde_json::to_string(&state) else {
        return;
    };
    if app.cards.set_agent_state(card_id, &state).await.is_ok() {
        crate::app::events::publish(crate::app::events::EventKind::Card);
    }
}

/// Posts the run's terminal output (result + transcript tail) to the card as
/// a comment, then tears the slot down. Runs on every work-tree exit path,
/// publish success or not.
async fn teardown(app: &TaskApp, card_id: i64, wt: &WorkTree, slot: &str, agent: &str) {
    match wt.finish(slot).await {
        Ok(out) => {
            let transcript = transcript_of(&out);
            if let Err(e) = app
                .store
                .add_comment(
                    card_id,
                    agent,
                    &output_comment(out.result.as_deref(), &transcript),
                )
                .await
            {
                tracing::warn!(error = %e, "agent output comment failed");
            }
        }
        Err(e) => tracing::warn!(error = %e, "work tree teardown failed"),
    }
}

fn output_comment(result: Option<&str>, transcript: &str) -> String {
    let mut body = result.unwrap_or_default().to_owned();
    if !transcript.is_empty() {
        if !body.is_empty() {
            body.push_str("\n\n");
        }
        body.push_str("terminal:\n");
        body.push_str(&tail(transcript, OUTPUT_COMMENT_MAX));
    }
    body
}

fn transcript_of(outcome: &TaskOutcome) -> String {
    outcome
        .state
        .history
        .iter()
        .map(|e| format!("{:?}: {}", e.role, e.content))
        .collect::<Vec<_>>()
        .join("\n")
}

fn call_label(call: &ToolCall) -> String {
    match call {
        ToolCall::Shell(cmd) => format!("SHELL {cmd}"),
        ToolCall::AgentRun { cmd, .. } => format!("AGENT_RUN {cmd}"),
        ToolCall::Git { op, .. } => format!("GIT {op:?}"),
        ToolCall::Coding { path, .. } => format!("CODING {path}"),
        other => format!("{other:?}"),
    }
}

/// One transcript turn into the kat pruner: cold, old tool outputs are
/// evicted first once the token budget is exceeded.
fn append_tool(transcript: &mut ToolPruner, label: &str, out: &str) {
    transcript.append(label, &truncate(out, TOOL_OUTPUT_MAX));
}

/// Corrective feedback for a malformed tool offer: a stuck/invalid JSON call
/// gets the specific kat repair prompt, anything else the generic reminder.
fn retry_note(reply: &str) -> String {
    match check_json(reply) {
        Ok(value) => {
            let bad = latenspace::kat_tool_call::global_policy().violations(&value);
            if bad.is_empty() {
                CARD_TOOL_MALFORMED.to_owned()
            } else {
                latenspace::kat_tool_call::policy_prompt(&bad)
            }
        }
        Err(
            stuck
            @ (JsonStuck::Truncated { .. } | JsonStuck::Invalid { .. } | JsonStuck::NotObject),
        ) => repair_prompt(&stuck),
        _ => CARD_TOOL_MALFORMED.to_owned(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Last `max` chars — evidence keeps the END of a log, where the reason
/// lives, not the start.
pub fn tail(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_owned();
    }
    chars[chars.len() - max..].iter().collect()
}

/// Whether a GIT DIFF output carries real work: the tool renders an empty
/// patch as the [`NO_CHANGES`] placeholder, which is clean, not content.
pub fn diff_is_dirty(diff_out: &str) -> bool {
    let t = diff_out.trim();
    !t.is_empty() && t != NO_CHANGES
}

/// Recovery for models that drop the `TOOL: ` prefix: the first line that
/// starts with a bare work verb is rewritten as a tool call and handed to
/// the strict parser. AGENT_RUN becomes SHELL — in a card work run it is
/// documented as an alias of the in-tree shell, not a remote agent run.
/// Returns None when the strict parser would find the call itself or
/// nothing looks like a tool line.
pub fn lenient_tool(reply: &str) -> Option<ToolCall> {
    if ToolCall::offers(reply) {
        return None;
    }
    bare_verb_line(reply)
        .or_else(|| prompt_dollar_line(reply))
        .or_else(|| fenced_shell_line(reply))
        .map(|line| ToolCall::parse(&format!("{LENIENT_PREFIX}{line}")))?
}

fn bare_verb_line(reply: &str) -> Option<String> {
    let line = reply.lines().find_map(|l| {
        let t = l.trim_start();
        let (verb, _) = t.split_once(' ')?;
        LENIENT_TOOLS
            .iter()
            .any(|k| verb.eq_ignore_ascii_case(k))
            .then_some(t)
    })?;
    let (verb, rest) = line.split_once(' ')?;
    if verb.eq_ignore_ascii_case("AGENT_RUN") {
        Some(format!("SHELL {rest}"))
    } else {
        Some(line.to_owned())
    }
}

fn prompt_dollar_line(reply: &str) -> Option<String> {
    let line = reply.lines().map(str::trim_start).find_map(|l| {
        l.strip_prefix("$ ")
            .filter(|cmd| !cmd.trim().is_empty())
            .map(str::to_owned)
    })?;
    Some(format!("SHELL {line}"))
}

const SHELL_FENCE_TAGS: [&str; 3] = ["sh", "bash", "shell"];
const FENCE: char = '`';

fn fenced_shell_line(reply: &str) -> Option<String> {
    let mut in_block = false;
    for line in reply.lines() {
        let t = line.trim();
        if in_block {
            if t.starts_with(FENCE) {
                in_block = false;
                continue;
            }
            let cmd = t.trim_start_matches("$ ").trim();
            if !cmd.is_empty() {
                return Some(format!("SHELL {cmd}"));
            }
        } else if let Some(t) = t.strip_prefix(FENCE) {
            in_block = SHELL_FENCE_TAGS.contains(&t.trim());
        }
    }
    None
}

async fn exec_tool(wt: &WorkTree, bound: &BoundRepo, work_tree: &Path, call: ToolCall) -> String {
    match call {
        ToolCall::Shell(cmd) => tool_out(wt.run(&bound.slot, &cmd).await),
        ToolCall::AgentRun { cmd, .. } => tool_out(wt.run(&bound.slot, &cmd).await),
        // Branch moves are publish-automation territory: a model-invented
        // branch strands commits off the task branch and dead-ends the PR.
        ToolCall::Git {
            op: GitOp::Branch { .. },
            ..
        } => NOTE_BRANCH_DENIED.to_owned(),
        ToolCall::Git { op, .. } => match ManagerGit::to_proto(&bind_op(op, bound)) {
            Ok(tool) => tool_out(wt.git(&bound.slot, tool).await),
            Err(e) => e,
        },
        ToolCall::Coding { path, code } => match resolve_in_tree(work_tree, &path) {
            Ok(target) => match write_new_file(&target, &code).await {
                Ok(()) => "file written".to_owned(),
                Err(e) => format!("{NOTE_WRITE_ERR}{e}"),
            },
            Err(e) => e,
        },
        _ => DENIED_NOTE.to_owned(),
    }
}

fn tool_out(result: Result<String, String>) -> String {
    result.unwrap_or_else(|e| e)
}

/// Writes a file into the work tree, creating missing parent directories —
/// models routinely target new package paths that do not exist yet.
async fn write_new_file(target: &Path, code: &str) -> std::io::Result<()> {
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(target, code).await
}

/// Resolves a model-supplied path inside the slot work tree; only normal
/// relative components pass (mirrors the podman Runner write_file contract).
pub fn resolve_in_tree(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(rel);
    let mut target = root.to_path_buf();
    for comp in rel_path.components() {
        match comp {
            Component::Normal(part) => target.push(part),
            _ => return Err(format!("{PATH_ESCAPE}{rel}")),
        }
    }
    Ok(target)
}

/// Fills url-less clone/push/pr ops with the bound repo, like chat does.
pub fn bind_op(op: GitOp, bound: &BoundRepo) -> GitOp {
    let token = |t: Option<String>| {
        t.filter(|s| !s.is_empty())
            .or_else(|| (!bound.token.is_empty()).then(|| bound.token.clone()))
    };
    match op {
        GitOp::Clone { url, token: t } => GitOp::Clone {
            url: Some(url.unwrap_or_else(|| bound.url.clone())),
            token: token(t),
        },
        GitOp::Push {
            branch,
            url,
            token: t,
        } => GitOp::Push {
            branch,
            url: Some(url.unwrap_or_else(|| bound.url.clone())),
            token: token(t),
        },
        GitOp::PullRequest {
            title,
            head,
            base,
            url,
            token: t,
        } => GitOp::PullRequest {
            title,
            head,
            base,
            url: Some(url.unwrap_or_else(|| bound.url.clone())),
            token: token(t),
        },
        other => other,
    }
}
