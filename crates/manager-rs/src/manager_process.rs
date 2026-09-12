//! Manager process: owns one sandboxed work tree per agent. Spawns an agent
//! sandbox on demand, runs the agent's commands in it, and on task finish
//! returns the result plus the sandbox state (cwd + transcript) before teardown.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use core_agent::podman::Sandbox;
use core_agent::sandbox_abstract_layer::{Role, SandboxState};
use git_rs::GitRepo;
use serde::Serialize;

/// Root directory holding every agent work tree.
pub const AGENTS_ROOT: &str = "work/agents";
pub const EMPTY_PATCH: &str = "";

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
}

struct AgentSlot {
    sandbox: Sandbox,
    work_tree: PathBuf,
    runs: AtomicU64,
    last_result: RwLock<Option<String>>,
    /// HEAD when the agent was spawned: base for the task diff.
    base_commit: Option<String>,
}

pub struct ManagerProcess {
    agents: RwLock<HashMap<String, Arc<AgentSlot>>>,
}

impl ManagerProcess {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            agents: RwLock::new(HashMap::new()),
        })
    }

    /// Spawns (or reuses) the sandboxed work tree for `agent`. A stale
    /// non-empty tree (crashed run, backend restart) holds no live agent, so
    /// it is reclaimed before the fresh sandbox is created.
    pub fn spawn(&self, agent: &str) -> Result<PathBuf, String> {
        let mut agents = self
            .agents
            .write()
            .map_err(|_| "agent map poisoned".to_string())?;
        if let Some(slot) = agents.get(agent) {
            return Ok(slot.work_tree.clone());
        }
        let work_tree = PathBuf::from(AGENTS_ROOT).join(sanitize(agent));
        reclaim_stale(&work_tree);
        let sandbox = Sandbox::new_in(&work_tree).map_err(|e| e.to_string())?;
        let base_commit = GitRepo::open_or_init(&work_tree)
            .ok()
            .and_then(|repo| repo.head_oid().ok())
            .map(|oid| oid.to_string());
        agents.insert(
            agent.to_string(),
            Arc::new(AgentSlot {
                sandbox,
                work_tree: work_tree.clone(),
                runs: AtomicU64::new(0),
                last_result: RwLock::new(None),
                base_commit,
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

    /// Finishes the agent's task: captures the task patch (committing pending
    /// work first), returns result + sandbox state (cwd and transcript), then
    /// tears the sandbox and its work tree down.
    pub fn finish(&self, agent: &str) -> Result<TaskOutcome, String> {
        let slot = self.slot(agent)?;
        let (patch, commit) = capture_patch(&slot.work_tree, &slot.base_commit, agent);
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
