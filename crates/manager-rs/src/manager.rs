//! Manager process: owns one sandboxed work tree per agent. Spawns an agent
//! sandbox on demand, runs the agent's commands in it, and on task finish
//! returns the result plus the sandbox state (cwd + transcript) before teardown.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use core_agent::podman::{Sandbox, cached_tag, resolve_image};
use core_agent::sandbox_abstract_layer::{Role, SandboxState};
use git_rs::GitRepo;
use proto_rs::GitTool;
use serde::Serialize;

use crate::git_state;

/// Root directory holding every agent work tree.
pub const AGENTS_ROOT: &str = "work/agents";
pub const EMPTY_PATCH: &str = "";
/// Separator between agent name and task id in a composite slot key.
const TASK_KEY_SEP: char = '#';
/// Git branch prefix for task-scoped agent work.
const TASK_BRANCH_PREFIX: &str = "task/";

/// Remote repo an agent work tree starts from; cloned on first spawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRepo {
    pub url: String,
    pub token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentRunState {
    Running,
    Finished,
}

#[derive(Serialize)]
pub struct AgentInfo {
    pub agent: String,
    pub work_tree: PathBuf,
    pub runs: u64,
    /// Last command the agent ran (from its transcript), if any.
    pub last_cmd: Option<String>,
}

#[derive(Serialize)]
pub struct AgentLogs {
    pub agent: String,
    pub work_tree: PathBuf,
    pub runs: u64,
    pub transcript: Vec<String>,
    pub last_result: Option<String>,
}

/// Result of publishing one task branch: push + PR.
#[derive(Debug, Serialize)]
pub struct PublishInfo {
    pub branch: String,
    pub push: String,
    pub pr: String,
}

#[derive(Serialize)]
pub struct TaskOutcome {
    pub agent: String,
    pub result: Option<String>,
    pub state: SandboxState,
    pub work_tree: PathBuf,
    pub patch: String,
    pub commit: Option<String>,
    /// Task branch the work landed on, when the slot was task-scoped.
    pub branch: Option<String>,
}

struct AgentSlot {
    sandbox: Sandbox,
    work_tree: PathBuf,
    runs: AtomicU64,
    last_result: RwLock<Option<String>>,
    /// HEAD when the agent was spawned: base for the task diff.
    base_commit: Option<String>,
    /// Task branch created at spawn, when the slot is task-scoped.
    branch: Option<String>,
}

pub struct Manager {
    agents: RwLock<HashMap<String, Arc<AgentSlot>>>,
}

impl Manager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            agents: RwLock::new(HashMap::new()),
        })
    }

    /// Spawns (or reuses) the sandboxed work tree for `agent`. A stale
    /// non-empty tree (crashed run, backend restart) holds no live agent, so
    /// it is reclaimed before the fresh sandbox is created.
    pub fn spawn(&self, agent: &str) -> Result<PathBuf, String> {
        self.spawn_task_impl(agent, None, None)
    }

    /// Like [`spawn`](Self::spawn), but seeds a fresh work tree by cloning
    /// `repo` (token kept out of on-disk config). An existing tree is reused
    /// as-is — the repo only applies to the first spawn.
    pub fn spawn_with_repo(
        &self,
        agent: &str,
        repo: Option<&RemoteRepo>,
    ) -> Result<PathBuf, String> {
        self.spawn_task_impl(agent, None, repo)
    }

    /// Spawns a dedicated work tree + sandbox for one task of `agent`. Two
    /// tasks of the same agent never share a slot. When the tree holds a
    /// repo with commits, a task branch is created and checked out so all
    /// task work lands isolated from other tasks.
    ///
    /// The returned path doubles as the slot key: [`run`](Self::run),
    /// [`finish`](Self::finish) and friends address the slot by it.
    pub fn spawn_task(&self, agent: &str, task: &str) -> Result<PathBuf, String> {
        self.spawn_task_impl(agent, Some(task), None)
    }

    /// Like [`spawn_task`](Self::spawn_task), but seeds the fresh work tree
    /// by cloning `repo`. This is the card-run entry point: one container +
    /// one task branch per (agent, task) pair.
    pub fn spawn_task_with_repo(
        &self,
        agent: &str,
        task: &str,
        repo: &RemoteRepo,
    ) -> Result<PathBuf, String> {
        self.spawn_task_impl(agent, Some(task), Some(repo))
    }

    fn spawn_task_impl(
        &self,
        agent: &str,
        task: Option<&str>,
        repo: Option<&RemoteRepo>,
    ) -> Result<PathBuf, String> {
        let key = slot_key(agent, task);
        if let Some(slot) = self
            .agents
            .read()
            .map_err(|_| "agent map poisoned".to_string())?
            .get(&key)
        {
            return Ok(slot.work_tree.clone());
        }
        // Heavy setup (sandbox create, repo seed — a clone can block on the
        // network) runs WITHOUT the agents write lock held; a snapshot()
        // must never wait on it.
        let work_tree = PathBuf::from(AGENTS_ROOT).join(sanitize(&key));
        reclaim_stale(&work_tree);
        // Run from the agent's cached image when one exists (installs from a
        // previous task), else the default coding image; every run is then
        // committed back into the cache tag for the next spawn.
        let cache = cached_tag(agent);
        let sandbox = Sandbox::new_in_with_image(&work_tree, &resolve_image(agent), Some(cache))
            .map_err(|e| e.to_string())?;
        // The container mounts sandbox.root() at /workspace — the repo must
        // live there, or in-sandbox branch/commit/push never see it.
        let workspace = sandbox.root();
        seed_work_tree(&workspace, repo)?;
        let branch = task.map(|t| task_branch_name(agent, t)).and_then(|name| {
            create_task_branch(&workspace, &name)?;
            Some(name)
        });
        let base_commit = GitRepo::open(&workspace)
            .ok()
            .and_then(|repo| repo.head_oid().ok())
            .map(|oid| oid.to_string());
        let mut agents = self
            .agents
            .write()
            .map_err(|_| "agent map poisoned".to_string())?;
        if let Some(slot) = agents.get(&key) {
            return Ok(slot.work_tree.clone());
        }
        agents.insert(
            key,
            Arc::new(AgentSlot {
                sandbox,
                work_tree: work_tree.clone(),
                runs: AtomicU64::new(0),
                last_result: RwLock::new(None),
                base_commit,
                branch,
            }),
        );
        Ok(work_tree)
    }

    /// Runs one command as `agent` inside its sandbox; the output is kept as
    /// the agent's pending result and the command appended to its transcript.
    pub fn run(&self, agent: &str, cmd: &str) -> Result<String, String> {
        let slot = self.slot(agent)?;
        let output = slot.sandbox.run(cmd).map_err(|e| e.to_string())?;
        slot.sandbox
            .push_context(Role::Agent, format!("$ {cmd}\n{output}"));
        slot.runs.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut last) = slot.last_result.write() {
            *last = Some(output.clone());
        }
        Ok(output)
    }

    /// Records `text` as agent context without executing anything.
    pub fn push_context(&self, agent: &str, text: &str) -> Result<(), String> {
        self.slot(agent)?.sandbox.push_context(Role::User, text);
        Ok(())
    }

    /// Runs a git toolcall for `agent` (spawning the agent on demand).
    /// Clone/status/diff run host-side in the work tree; branch/commit/push/
    /// pr run inside the agent's own container with network + run-scoped
    /// token env.
    pub fn git_tool(&self, agent: &str, tool: &GitTool) -> Result<String, String> {
        // The first read guard must drop before spawning: re-entering the
        // lock while a queued writer waits deadlocks on itself.
        let cached = self
            .agents
            .read()
            .map_err(|_| "agent map poisoned".to_string())?
            .get(agent)
            .cloned();
        let slot = match cached {
            Some(slot) => slot,
            None => {
                self.spawn(agent)?;
                self.agents
                    .read()
                    .map_err(|_| "agent map poisoned".to_string())?
                    .get(agent)
                    .cloned()
                    .ok_or_else(|| format!("agent {agent} failed to spawn"))?
            }
        };
        match tool {
            GitTool::Branch { .. }
            | GitTool::Commit { .. }
            | GitTool::Push { .. }
            | GitTool::PullRequest { .. } => {
                core_agent::toolcall::git_in_sandbox::apply(&slot.sandbox, tool)
            }
            GitTool::Clone { .. } | GitTool::Status | GitTool::Diff => {
                git_state::apply(&slot.sandbox.root(), tool)
            }
        }
    }

    /// Publishes a task slot's work: commits pending changes, pushes the
    /// task branch to `repo_url` (default `origin`) and opens a PR against
    /// `base` (default [`PR_BASE_DEFAULT`]) via the GitHub API. Needs a
    /// task-scoped slot — the branch to publish is the one created at spawn.
    /// `pr_repo` overrides the repo url used for the PR (slug source) when
    /// the push remote is not the github repo itself.
    pub fn publish(
        &self,
        agent: &str,
        repo_url: Option<&str>,
        token: &str,
        base: Option<&str>,
        pr_repo: Option<&str>,
    ) -> Result<PublishInfo, String> {
        let slot = self.slot(agent)?;
        let branch = slot
            .branch
            .clone()
            .ok_or_else(|| format!("agent {agent} has no task branch to publish"))?;
        // Commit pending work so the push carries everything the task did.
        if let Ok(repo) = GitRepo::open(&slot.sandbox.root()) {
            repo.task_patch(slot.base_commit.as_deref(), agent)
                .map_err(|e| format!("commit before push: {e}"))?;
        }
        let url = repo_url.map(str::to_owned);
        let token = Some(token.to_owned());
        let push = core_agent::toolcall::git_in_sandbox::apply(
            &slot.sandbox,
            &GitTool::Push {
                branch: branch.clone(),
                url: url.clone(),
                token: token.clone(),
            },
        )?;
        let pr = core_agent::toolcall::git_in_sandbox::apply(
            &slot.sandbox,
            &GitTool::PullRequest {
                title: branch.clone(),
                head: branch.clone(),
                base: base
                    .unwrap_or(core_agent::toolcall::git_in_sandbox::PR_BASE_DEFAULT)
                    .to_owned(),
                url: pr_repo.map(str::to_owned).or_else(|| url.clone()),
                token,
            },
        )?;
        Ok(PublishInfo { branch, push, pr })
    }

    /// Finishes the agent's task: captures the task patch (committing pending
    /// work first), returns result + sandbox state (cwd and transcript), then
    /// tears the sandbox and its work tree down.
    pub fn finish(&self, agent: &str) -> Result<TaskOutcome, String> {
        let slot = self.slot(agent)?;
        let (patch, commit) = capture_patch(&slot.sandbox.root(), &slot.base_commit, agent);
        let state = SandboxState {
            cwd: slot
                .sandbox
                .transcript()
                .last()
                .map(|_| slot.sandbox.root().to_string_lossy().into_owned())
                .unwrap_or_else(|| ".".into()),
            history: slot.sandbox.transcript(),
        };
        let outcome = TaskOutcome {
            agent: agent.to_string(),
            result: slot.last_result.read().ok().and_then(|r| r.clone()),
            state,
            work_tree: slot.work_tree.clone(),
            patch,
            commit,
            branch: slot.branch.clone(),
        };
        slot.sandbox.purge();
        if let Ok(mut agents) = self.agents.write() {
            agents.remove(agent);
        }
        Ok(outcome)
    }

    pub fn logs(&self, agent: &str) -> Result<AgentLogs, String> {
        let slot = self.slot(agent)?;
        let transcript = slot
            .sandbox
            .transcript()
            .into_iter()
            .map(|e| format!("{:?}: {}", e.role, e.content))
            .collect();
        Ok(AgentLogs {
            agent: agent.to_string(),
            work_tree: slot.work_tree.clone(),
            runs: slot.runs.load(Ordering::Relaxed),
            transcript,
            last_result: slot.last_result.read().ok().and_then(|r| r.clone()),
        })
    }

    pub fn snapshot(&self) -> Vec<AgentInfo> {
        let Ok(agents) = self.agents.read() else {
            return Vec::new();
        };
        let mut infos: Vec<AgentInfo> = agents
            .iter()
            .map(|(name, slot)| AgentInfo {
                agent: name.clone(),
                work_tree: slot.work_tree.clone(),
                runs: slot.runs.load(Ordering::Relaxed),
                last_cmd: slot.sandbox.transcript().iter().rev().find_map(|e| {
                    if !matches!(e.role, Role::Agent) {
                        return None;
                    }
                    // run() records commands as "$ <cmd>\n<output>".
                    e.content
                        .lines()
                        .next()
                        .and_then(|l| l.strip_prefix("$ "))
                        .map(str::to_owned)
                }),
            })
            .collect();
        infos.sort_by(|a, b| a.agent.cmp(&b.agent));
        infos
    }

    fn slot(&self, agent: &str) -> Result<Arc<AgentSlot>, String> {
        self.agents
            .read()
            .map_err(|_| "agent map poisoned".to_string())?
            .get(agent)
            .cloned()
            .ok_or_else(|| format!("agent {agent} not spawned"))
    }
}

/// Clones `repo` into the fresh work tree; falls back to an empty init repo
/// so a broken remote never blocks the agent from starting.
fn seed_work_tree(work_tree: &Path, repo: Option<&RemoteRepo>) -> Result<(), String> {
    let Some(repo) = repo else {
        return GitRepo::open_or_init(work_tree)
            .map(|_| ())
            .map_err(|e| e.to_string());
    };
    match GitRepo::clone_into(&repo.url, work_tree, repo.token.as_deref()) {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("[manager] clone {} failed ({e}); starting empty", repo.url);
            let _ = fs::remove_dir_all(work_tree);
            GitRepo::open_or_init(work_tree)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
    }
}

fn sanitize(agent: &str) -> String {
    agent
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Composite map key: one slot per (agent, task) pair; task-less spawns
/// keep the bare agent name as their key.
fn slot_key(agent: &str, task: Option<&str>) -> String {
    match task {
        Some(t) => format!("{agent}{TASK_KEY_SEP}{t}"),
        None => agent.to_string(),
    }
}

/// Branch name for one task of `agent`, sanitized for git ref safety.
fn task_branch_name(agent: &str, task: &str) -> String {
    format!("{TASK_BRANCH_PREFIX}{}-{}", sanitize(task), sanitize(agent))
}

/// Creates + checks out `branch` in the freshly seeded work tree. Best
/// effort: a branch failure must not block the agent from starting.
fn create_task_branch(work_tree: &Path, branch: &str) -> Option<()> {
    let repo = GitRepo::open(work_tree).ok()?;
    if !repo.has_commits() {
        return None;
    }
    repo.create_checkout_branch(branch)
        .map(|_| ())
        .map_err(|e| eprintln!("[manager] task branch {branch}: {e}"))
        .ok()
}

/// Best-effort task patch capture; teardown must proceed even if git fails.
fn capture_patch(work_tree: &Path, base: &Option<String>, agent: &str) -> (String, Option<String>) {
    let captured =
        GitRepo::open(work_tree).and_then(|repo| repo.task_patch(base.as_deref(), agent));
    match captured {
        Ok(task) => (task.patch, task.commit),
        Err(e) => {
            eprintln!("[manager] patch capture failed for {agent}: {e}");
            (EMPTY_PATCH.to_string(), None)
        }
    }
}

/// A work tree is empty when it does not exist or holds no entries.
fn is_empty_dir(path: &Path) -> bool {
    fs::read_dir(path)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(true)
}

fn reclaim_stale(work_tree: &Path) {
    if !is_empty_dir(work_tree) {
        let _ = fs::remove_dir_all(work_tree);
    }
}
