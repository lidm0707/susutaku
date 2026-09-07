//! Sandbox adapter: agent shell tool backed by core_agent's macOS sandbox.

use crate::port::outbound::Runner;
use core_agent::sandbox::{Sandbox, SandboxDir};
use std::path::PathBuf;

pub struct AgentSandbox {
    inner: Sandbox,
}

impl AgentSandbox {
    pub fn restore() -> Result<Self, String> {
        Sandbox::purge_stale();
        Sandbox::restore()
            .map(|inner| Self { inner })
            .map_err(|e| e.to_string())
    }

    /// Workspace root of the underlying macOS sandbox.
    pub fn root(&self) -> PathBuf {
        self.inner.root()
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
        self.inner.run(cmd).map_err(|e| e.to_string())
    }
}
