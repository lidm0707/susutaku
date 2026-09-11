//! Sandbox adapter: agent shell tool backed by core_agent's macOS sandbox.

use crate::port::outbound::Runner;
use core_agent::sandbox::{Sandbox, SandboxDir};
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
        core_agent::sandbox::list_dirs()
    }

    /// Delete another (ideally dead) backend's sandbox dir.
    pub fn purge_dir(pid: u32) -> Result<bool, String> {
        core_agent::sandbox::purge_dir(pid)
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
}
