//! Sandbox adapter: agent shell tool backed by core_agent's podman sandbox.

use crate::domain::GitOp;
use crate::port::outbound::Runner;
use core_agent::podman::{Sandbox, SandboxDir};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

const STEP_PREFIX: &str = "step";

pub struct AgentSandbox {
    inner: Sandbox,
    step: AtomicUsize,
}

impl AgentSandbox {
    /// Restore a saved sandbox, or create a fresh one on first boot (a new
    /// instance never has saved state — e.g. a freshly started container).
    pub fn restore() -> Result<Self, String> {
        Sandbox::purge_stale();
        let inner = match Sandbox::restore() {
            Ok(inner) => inner,
            Err(_) => Sandbox::new().map_err(|e| e.to_string())?,
        };
        Ok(Self {
            inner,
            step: AtomicUsize::new(0),
        })
    }

    /// Workspace root of the underlying macOS sandbox.
    pub fn root(&self) -> PathBuf {
        self.inner.root()
    }

    /// Snapshot of the workspace state under `root/snapshots/<label>`.
    pub fn snapshot(&self, label: &str) -> Result<PathBuf, String> {
        self.inner.snapshot(label).map_err(|e| e.to_string())
    }

    /// All sandbox dirs with owning-process liveness.
    pub fn dirs() -> Vec<SandboxDir> {
        core_agent::podman::list_dirs()
    }

    /// Delete another (ideally dead) backend's sandbox dir.
    pub fn purge_dir(pid: u32) -> Result<bool, String> {
        core_agent::podman::purge_dir(pid)
    }

    /// Delete dirs of dead PIDs; returns how many were removed.
    pub fn sweep() -> usize {
        let before = Self::dirs();
        Sandbox::purge_stale();
        before.iter().filter(|d| !d.alive).count()
    }
}

impl Runner for AgentSandbox {
    fn run(&self, cmd: &str) -> Result<String, String> {
        // Per-step snapshot before the command runs: the pre-step workspace
        // can be diffed/restored after the fact. Best-effort: a snapshot
        // failure must not block the agent's shell step.
        let step = self.step.fetch_add(1, Ordering::Relaxed);
        if let Err(e) = self.inner.snapshot(&format!("{STEP_PREFIX}-{step:04}")) {
            eprintln!("sandbox: pre-step snapshot failed: {e}");
        }
        self.inner.run(cmd).map_err(|e| e.to_string())
    }

    fn write_file(&self, path: &str, content: &str) -> Result<(), String> {
        let rel = workspace_rel_path(path)?;
        let target = self.inner.root().join(&rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&target, content).map_err(|e| e.to_string())
    }

    fn has_git_repo(&self) -> bool {
        self.inner.root().join(".git").exists()
    }

    fn git(&self, op: &GitOp) -> Result<String, String> {
        let tool = match op {
            GitOp::Clone { url, token } => proto_rs::GitTool::Clone {
                url: url.clone().ok_or_else(|| "clone needs a url".to_string())?,
                token: token.clone(),
            },
            GitOp::Status => proto_rs::GitTool::Status,
            GitOp::Diff => proto_rs::GitTool::Diff,
        };
        manager_rs::git_tool::apply(&self.inner.root(), &tool)
    }
}

/// Normalize a model-supplied path to a safe workspace-relative path:
/// absolute paths are rooted at the workspace, `..` escapes are rejected.
fn workspace_rel_path(path: &str) -> Result<String, String> {
    let trimmed = path.trim().trim_start_matches('/');
    let cleaned: Vec<&str> = trimmed
        .split('/')
        .filter(|seg| !seg.is_empty() && *seg != ".")
        .collect();
    if cleaned.contains(&"..") {
        return Err(format!("coding path escapes the work tree: {path}"));
    }
    if cleaned.is_empty() {
        return Err("coding path is empty".to_string());
    }
    Ok(cleaned.join("/"))
}
