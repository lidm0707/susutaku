//! Runs a card's assigned agent: the agent config (persona, instruction,
//! output format) is prepended to the card's title/description and submitted
//! to the model; the run ledger is merged into the card's agent state.
//!
//! Work-tree mode: when the card has an agent, a project with a bound git
//! repo and a [`WorkTree`] is wired in, the run executes against a real
//! manager task slot (model edits files via toolcalls), then publishes the
//! branch and opens a GitHub PR, storing the durable agent output.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use manager_rs::manager::{Manager, RemoteRepo, TaskOutcome};
use task_rs::store::{CardRow, RunRecordNew, StoreError};
use task_rs::{AgentConfigRow, SkillRow, TaskStatus};

use crate::domain::{CardMove, GitOp, TOOL_RESULT_HEADER, TOOL_ROUNDS_MAX, ToolCall};
use crate::infra::manager_git::ManagerGit;
use crate::infra::zai::settings::SettingsState;
use crate::port::outbound::{Inference, ModelEngines};

use super::task::TaskApp;

pub const RUN_KEY: &str = "run";
pub const MOVE_POSITION_TOP: i32 = 0;
pub const NOTE_NO_AGENT: &str = "no agent assigned to the card";
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
pub const CARD_TOOL_ROUNDS: usize = TOOL_ROUNDS_MAX;
pub const WORK_MAX_TOKENS: usize = 2048;
pub const TOOL_OUTPUT_MAX: usize = 2000;
pub const SLOT_KEY_SEP: char = manager_rs::manager::TASK_KEY_SEP;
pub const NOTE_SPAWN_ERR: &str = "work tree spawn failed: ";
pub const NOTE_PUBLISH_ERR: &str = "publish failed: ";
pub const NOTE_WRITE_ERR: &str = "file write failed: ";
pub const NOTE_JOIN: &str = "manager task panicked";
pub const DENIED_NOTE: &str =
    "tool denied in a card run: use SHELL, AGENT_RUN, GIT or the coding write_file block";
pub const TOOL_HINT: &str = "You are working alone in a sandboxed work tree of the project's git repo (a task branch is checked out). Edit real files, one tool call per reply. Every tool line MUST start with exactly 'TOOL: ' — a line that only says 'SHELL ...' is NOT a tool call and ends the run:\n- TOOL: SHELL <cmd> - run a shell command in the work tree\n- TOOL: AGENT_RUN <cmd> - run a shell command in the work tree (alias)\n- TOOL: GIT STATUS | DIFF | BRANCH <name> | COMMIT <message> | PUSH <branch> | PR <title>\n- coding block: <invoke name=\"coding\"><parameter name=\"path\">rel/path</parameter><parameter name=\"code\">file content</parameter></invoke>\nDo the actual work (create/edit files, verify with SHELL) before finishing. Read-only exploration alone is NOT a finished task — if you have not changed any files, keep working. When done, reply with a final text summary (no tool line); the run publishes the branch and opens a PR automatically.\n";
pub const PUBLISH_BRANCH: &str = "\n\nbranch: ";
pub const PUBLISH_PUSH: &str = "\npush: ";
pub const PUBLISH_PR: &str = "\npr: ";
pub const PUBLISH_PR_FAILED: &str = "\npr: failed: ";
pub const PUBLISH_NOTHING: &str = "\n\nnothing to publish: the run made no commits";
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
            m.spawn_task_with_repo(&agent, &task, &repo)
        })
        .await
        .map_err(|_| NOTE_JOIN.to_owned())?
    }

    pub async fn run(&self, slot: &str, cmd: &str) -> Result<String, String> {
        let (m, slot, cmd) = (self.manager.clone(), slot.to_owned(), cmd.to_owned());
        tokio::task::spawn_blocking(move || m.run(&slot, &cmd))
            .await
            .map_err(|_| NOTE_JOIN.to_owned())?
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
) -> Result<RunRecord, StoreError> {
    let card = app
        .cards
        .get(card_id)
        .await?
        .ok_or(StoreError::NoSuchCard)?;
    let agent = card.agent_name.clone().unwrap_or_default();
    if app.cards.set_run_start(card_id, &agent).await.is_ok() {
        crate::app::events::publish(crate::app::events::EventKind::Card);
    }
    move_to_column(app, card_id, &card.column_id, TaskStatus::InProgress).await?;
    let record = match select_mode(wt, &card) {
        RunMode::Text => execute(app, engine.as_ref(), engines, &card).await,
        RunMode::Work(bound) => {
            execute_work(
                app,
                engine.as_ref(),
                engines,
                &card,
                wt.expect("mode selected with work tree"),
                bound,
            )
            .await
        }
    };
    persist(app, card_id, &card, record, trigger).await
}

async fn execute(
    app: &TaskApp,
    engine: Option<&Arc<dyn Inference>>,
    engines: Option<&dyn ModelEngines>,
    card: &CardRow,
) -> RunRecord {
    let agent = card.agent_name.clone().unwrap_or_default();
    match (engine, card.agent_name.as_deref()) {
        (_, None) => fail(&agent, NOTE_NO_AGENT),
        (None, Some(_)) => fail(&agent, NOTE_NO_ENGINE),
        (Some(engine), Some(name)) => match app.agents.by_name(name).await.ok().flatten() {
            None => fail(&agent, NOTE_NO_AGENT),
            Some(cfg) => {
                let model = cfg.model.trim();
                let routed = engines
                    .and_then(|e| e.engine_for(model))
                    .filter(|_| !model.is_empty());
                let target = routed.as_ref().unwrap_or(engine);
                let skills = agent_skills(app, &cfg).await;
                infer(target, &cfg, card, &agent, &skills).await
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
    engine: &Arc<dyn Inference>,
    cfg: &AgentConfigRow,
    card: &CardRow,
    agent: &str,
    skills: &[SkillRow],
) -> RunRecord {
    match submit(
        engine,
        None,
        cfg,
        task_prompt(cfg, card, skills),
        INFER_MAX_TOKENS,
    )
    .await
    {
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

pub fn work_prompt(
    cfg: &AgentConfigRow,
    card: &CardRow,
    transcript: &str,
    skills: &[SkillRow],
) -> String {
    let mut prompt = task_prompt(cfg, card, skills);
    prompt.push_str(TEXT_SEP);
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
    match rx.await {
        Ok(Ok(reply)) => Ok(reply.text),
        Err(_) | Ok(Err(_)) => Err(NOTE_INFER_DROP.to_owned()),
    }
}

pub const NOTE_INFER_ERR: &str = "inference failed: ";
pub const NOTE_INFER_DROP: &str = "inference: engine dropped or rejected the job";

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
        .spawn_task(&bound.agent, &bound.slot, &bound.task, repo)
        .await
    {
        Ok(tree) => tree,
        Err(e) => return fail(&agent, &format!("{NOTE_SPAWN_ERR}{e}")),
    };
    let mut transcript = String::new();
    let mut final_text = String::new();
    for _ in 0..CARD_TOOL_ROUNDS {
        let prompt = work_prompt(&cfg, card, &transcript, &skills);
        let reply = match submit(engine, engines, &cfg, prompt, WORK_MAX_TOKENS).await {
            Ok(text) => text,
            Err(e) => return fail(&agent, &format!("{NOTE_INFER_ERR}{e}")),
        };
        match ToolCall::parse(&reply) {
            None => {
                final_text = reply;
                break;
            }
            Some(call) => {
                let label = call_label(&call);
                let out = exec_tool(wt, &bound, &work_tree, call).await;
                append_tool(&mut transcript, &label, &out);
            }
        }
    }
    let publish = publish_work(wt, &bound, &card.title).await;
    let mut output = final_text;
    match publish {
        Publish::Nothing => output.push_str(PUBLISH_NOTHING),
        Publish::Done(branch, push, pr) => {
            output.push_str(&publish_note(&branch, &push, &pr));
        }
        Publish::Failed(e) => {
            teardown(app, wt, &bound.slot, &agent).await;
            return fail(&agent, &format!("{NOTE_PUBLISH_ERR}{e}"));
        }
    }
    teardown(app, wt, &bound.slot, &agent).await;
    RunRecord {
        agent,
        status: RunStatus::Ok,
        output: Some(output),
        finished_at: now_iso(),
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
/// [`PR_BASE`]. A PR failure does not fail the run: the branch is already
/// on the remote and review can happen there.
async fn publish_work(wt: &WorkTree, bound: &BoundRepo, title: &str) -> Publish {
    // Gate on commits, not on a dirty diff: a work-dir diff can be non-empty
    // from runtime artifacts alone, and pushing with no new commits makes
    // the remote branch point at main — every PR then dies with
    // "No commits between main and <branch>".
    match wt.task_has_commits(&bound.slot).await {
        Ok(true) => {}
        Ok(false) => return Publish::Nothing,
        Err(e) => return Publish::Failed(e),
    }
    let diff = match ManagerGit::to_proto(&GitOp::Diff) {
        Ok(tool) => wt.git(&bound.slot, tool).await,
        Err(e) => return Publish::Failed(e),
    };
    match diff {
        Ok(d) if d.trim().is_empty() => return Publish::Nothing,
        Ok(_) => {}
        Err(e) => return Publish::Failed(e),
    }
    let branch = task_branch(&bound.agent, &bound.task);
    let commit = ManagerGit::to_proto(&GitOp::Commit {
        message: format!("{COMMIT_PREFIX}{title}"),
    });
    if let Err(e) = match commit {
        Ok(tool) => wt.git(&bound.slot, tool).await,
        Err(e) => return Publish::Failed(e),
    } {
        return Publish::Failed(format!("commit: {e}"));
    }
    let push = ManagerGit::to_proto(&GitOp::Push {
        branch: branch.clone(),
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
        title: branch.clone(),
        head: branch.clone(),
        base: PR_BASE.to_owned(),
        url: Some(bound.url.clone()),
        token: (!bound.token.is_empty()).then(|| bound.token.clone()),
    }) {
        Ok(tool) => wt.git(&bound.slot, tool).await,
        Err(e) => return Publish::Done(branch, push, format!("failed: {e}")),
    };
    match pr {
        Ok(out) => Publish::Done(branch, push, out),
        Err(e) => Publish::Done(branch, push, format!("failed: {e}")),
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

/// Captures the patch/transcript as a durable agent output, then tears the
/// slot down. Runs on every work-tree exit path, publish success or not.
async fn teardown(app: &TaskApp, wt: &WorkTree, slot: &str, agent: &str) {
    match wt.finish(slot).await {
        Ok(out) => {
            let transcript = transcript_of(&out);
            if let Err(e) = app
                .store
                .insert_agent_output(
                    agent,
                    out.result.as_deref(),
                    &out.patch,
                    out.commit.as_deref(),
                    &transcript,
                )
                .await
            {
                tracing::warn!(error = %e, "agent output store failed");
            }
        }
        Err(e) => tracing::warn!(error = %e, "work tree teardown failed"),
    }
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

fn append_tool(transcript: &mut String, label: &str, out: &str) {
    if transcript.is_empty() {
        transcript.push_str(TOOL_RESULT_HEADER);
    }
    transcript.push('\n');
    transcript.push_str(label);
    transcript.push('\n');
    transcript.push_str(&truncate(out, TOOL_OUTPUT_MAX));
    transcript.push('\n');
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

async fn exec_tool(wt: &WorkTree, bound: &BoundRepo, work_tree: &Path, call: ToolCall) -> String {
    match call {
        ToolCall::Shell(cmd) => tool_out(wt.run(&bound.slot, &cmd).await),
        ToolCall::AgentRun { cmd, .. } => tool_out(wt.run(&bound.slot, &cmd).await),
        ToolCall::Git { op, .. } => match ManagerGit::to_proto(&bind_op(op, bound)) {
            Ok(tool) => tool_out(wt.git(&bound.slot, tool).await),
            Err(e) => e,
        },
        ToolCall::Coding { path, code } => match resolve_in_tree(work_tree, &path) {
            Ok(target) => match tokio::fs::write(target, code).await {
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
