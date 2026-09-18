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
/// Root directory holding per-project bare repo caches that agent work
/// trees are spawned from as linked git worktrees (O(1) spawn, no clone).
pub const REPOS_ROOT: &str = "work/repos";
pub const EMPTY_PATCH: &str = "";
/// Separator between agent name and task id in a composite slot key.
pub const TASK_KEY_SEP: char = '#';
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
    /// Set when the finish request asked for a push: remote output, or the
    /// failure reason. The outcome (and artifact) is complete either way.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push: Option<Result<String, String>>,
}

/// Push request attached to a finish: push the task branch to the bound
/// repo before the work tree is torn down. `branch` overrides the slot's
/// task branch (or current branch) when given.
#[derive(Debug, Clone)]
pub struct FinishPush {
    pub repo_url: Option<String>,
    pub token: String,
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
    /// Bare cache repo this slot's worktree belongs to, when the slot was
    /// seeded from the cache (None = plain tree or legacy clone).
    repo_cache: Option<PathBuf>,
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
        // live there, or host-side git ops never see it.
        let workspace = sandbox.root();
        let branch_name = task.map(|t| task_branch_name(agent, t));
        let (branch, repo_cache) = seed_work_tree(&workspace, repo, branch_name.as_deref())?;
        let branch = branch.or_else(|| {
            task.and_then(|_| {
                branch_name
                    .as_deref()
                    .and_then(|name| create_task_branch(&workspace, name))
            })
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
                repo_cache,
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
    /// Every op runs host-side in the work tree — the repo token is used on
    /// the host only and never enters the agent's container.
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
        git_state::apply(&slot.sandbox.root(), tool)
    }

    /// Whether the slot's HEAD advanced past the base commit recorded at
    /// spawn — i.e. this task produced at least one commit. Publish (push
    /// and PR) must be gated on this: a dirty work-dir diff alone can come
    /// from runtime artifacts, and pushing without new commits makes the
    /// remote branch point at main, which dead-ends every PR with
    /// "No commits between main and <branch>".
    pub fn task_has_commits(&self, agent: &str) -> Result<bool, String> {
        let slot = self.slot(agent)?;
        let Some(base) = slot.base_commit.as_deref() else {
            return Ok(true);
        };
        let repo = GitRepo::open(&slot.sandbox.root()).map_err(|e| e.to_string())?;
        let head = repo.head_oid().map_err(|e| e.to_string())?;
        Ok(head.to_string() != base)
    }

    /// Finishes the agent's task: captures the task patch (committing pending
    /// work first), returns result + sandbox state (cwd and transcript), then
    /// tears the sandbox and its work tree down.
    pub fn finish(&self, agent: &str) -> Result<TaskOutcome, String> {
        self.finish_with_push(agent, None)
    }

    /// Like [`Manager::finish`], but optionally pushes the work to the
    /// remote first — the last step before the work tree disappears, so a
    /// commit can never be stranded on a deleted tree. Push failure does not
    /// fail the finish: the patch artifact is already captured and is
    /// reported in `TaskOutcome::push`.
    pub fn finish_with_push(
        &self,
        agent: &str,
        push: Option<FinishPush>,
    ) -> Result<TaskOutcome, String> {
        let slot = self.slot(agent)?;
        let (patch, commit) = capture_patch(&slot.sandbox.root(), &slot.base_commit, agent);
        let push_result = push.map(|p| self.push_before_teardown(&slot, &p));
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
            push: push_result,
        };
        slot.sandbox.purge();
        teardown_work_tree(&slot);
        if let Ok(mut agents) = self.agents.write() {
            agents.remove(agent);
        }
        Ok(outcome)
    }

    /// Pushes the branch the task's commits landed on. Chooses the explicit
    /// override, else the task branch, else the current branch; a repo with
    /// no commits has nothing to push and is reported as such.
    fn push_before_teardown(&self, slot: &AgentSlot, push: &FinishPush) -> Result<String, String> {
        let repo = GitRepo::open(&slot.sandbox.root()).map_err(|e| e.to_string())?;
        if !repo.has_commits() {
            return Ok("(nothing to push: no commits)".to_string());
        }
        let branch = match push.branch.as_deref().filter(|b| !b.is_empty()) {
            Some(b) => b.to_owned(),
            None => slot
                .branch
                .clone()
                .unwrap_or_else(|| repo.current_branch().unwrap_or_else(|_| "main".to_owned())),
        };
        let url = push
            .repo_url
            .clone()
            .ok_or("finish push needs the project's bound repo url")?;
        let token = (!push.token.is_empty()).then(|| push.token.clone());
        git_state::push(&slot.sandbox.root(), &branch, Some(&url), token.as_deref())
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

/// Seeds the fresh work tree. With a bound repo, the slot becomes a linked
/// git worktree of a per-project bare cache (`work/repos/`) — instant spawn,
/// no clone — and the task branch is created in the cache at the fresh HEAD.
/// Falls back to a plain clone, then to an empty init repo, so a broken
/// remote never blocks the agent from starting. Returns the task branch
/// (when created during seeding) and the cache path (when cache-backed).
fn seed_work_tree(
    work_tree: &Path,
    repo: Option<&RemoteRepo>,
    task_branch: Option<&str>,
) -> Result<(Option<String>, Option<PathBuf>), String> {
    let Some(repo) = repo else {
        GitRepo::open_or_init(work_tree)
            .map(|_| ())
            .map_err(|e| e.to_string())?;
        return Ok((None, None));
    };
    match seed_from_cache(work_tree, repo, task_branch) {
        Ok(branch) => return Ok((branch, Some(repo_cache_path(&repo.url)))),
        Err(e) => {
            eprintln!(
                "[manager] worktree seed from {} failed ({e}); falling back to clone",
                repo.url
            );
        }
    }
    match GitRepo::clone_into(&repo.url, work_tree, repo.token.as_deref()) {
        Ok(_) => Ok((None, None)),
        Err(e) => {
            eprintln!("[manager] clone {} failed ({e}); starting empty", repo.url);
            let _ = fs::remove_dir_all(work_tree);
            GitRepo::open_or_init(work_tree)
                .map(|_| ())
                .map_err(|e| e.to_string())?;
            Ok((None, None))
        }
    }
}

/// Bare cache mirror path for one repo url.
fn repo_cache_path(url: &str) -> PathBuf {
    PathBuf::from(REPOS_ROOT).join(format!("{}.git", sanitize(url)))
}

/// Ensures the bare cache for `repo.url` is fresh (clone once, force-fetch
/// after), then adds the slot's worktree from its HEAD. HEAD must resolve:
/// an empty or unusable cache is an error → the caller falls back to clone.
fn seed_from_cache(
    work_tree: &Path,
    repo: &RemoteRepo,
    task_branch: Option<&str>,
) -> Result<Option<String>, String> {
    let cache_path = repo_cache_path(&repo.url);
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let cache = match GitRepo::open(&cache_path) {
        Ok(existing) => {
            existing
                .fetch(&repo.url, repo.token.as_deref())
                .map_err(|e| e.to_string())?;
            existing
        }
        Err(_) => {
            let _ = fs::remove_dir_all(&cache_path);
            GitRepo::init_bare(&cache_path).map_err(|e| e.to_string())?;
            let fresh = GitRepo::open(&cache_path).map_err(|e| e.to_string())?;
            fresh
                .fetch(&repo.url, repo.token.as_deref())
                .map_err(|e| e.to_string())?;
            fresh
        }
    };
    if !cache.has_commits() {
        return Err("cache has no commits after fetch".to_string());
    }
    // A remote whose default branch is not `main` leaves the bare HEAD
    // dangling; point it at a branch that exists so worktree_add resolves.
    if !cache.head_resolves()
        && let Some(name) = cache.first_local_branch()
    {
        cache.set_head_to_branch(&name).map_err(|e| e.to_string())?;
    }
    // Stale metadata from reclaimed slots would block branch recreation.
    cache.worktree_prune().map_err(|e| e.to_string())?;
    // libgit2's worktree add refuses an existing target dir (MKDIR_EXCL);
    // the sandbox layout leaves it empty, so drop it and let git create it.
    // The podman bind only needs the dir at run time.
    fs::remove_dir(work_tree).map_err(|e| e.to_string())?;
    cache
        .worktree_add(work_tree, task_branch)
        .map_err(|e| e.to_string())?;
    Ok(task_branch.map(str::to_owned))
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
fn create_task_branch(work_tree: &Path, branch: &str) -> Option<String> {
    let repo = GitRepo::open(work_tree).ok()?;
    if !repo.has_commits() {
        return None;
    }
    repo.create_checkout_branch(branch)
        .map(|_| branch.to_owned())
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
    if is_empty_dir(work_tree) {
        return;
    }
    // Crashed-run guard: don't silently discard uncommitted work — save the
    // pending diff next to the slot before reclaiming.
    if let Ok(repo) = GitRepo::open(work_tree)
        && let Ok(patch) = repo.patch_workdir()
        && !patch.is_empty()
        && let Some(parent) = work_tree.parent()
    {
        let save = parent.join(format!(
            "{}.reclaimed.patch",
            work_tree
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ));
        let _ = fs::write(&save, &patch);
        eprintln!(
            "[manager] reclaimed dirty slot {}; uncommitted work saved to {}",
            work_tree.display(),
            save.display()
        );
    }
    let _ = fs::remove_dir_all(work_tree);
}

/// Tears the slot's work tree down. Cache-backed slots are removed as linked
/// worktrees so the cache's admin metadata stays clean; the dir delete is
/// best-effort either way (the patch artifact is already captured).
fn teardown_work_tree(slot: &AgentSlot) {
    if let Some(cache) = &slot.repo_cache
        && let Ok(cache_repo) = GitRepo::open(cache)
    {
        match cache_repo.worktree_remove(&slot.sandbox.root(), true) {
            Ok(()) => return,
            Err(e) => eprintln!(
                "[manager] worktree remove for {}: {e}; pruning instead",
                slot.work_tree.display()
            ),
        }
        let _ = cache_repo.worktree_prune();
    }
    let _ = fs::remove_dir_all(&slot.work_tree);
}
