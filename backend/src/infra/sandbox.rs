//! Sandbox adapter: agent shell tool backed by core_agent's macOS sandbox.

use crate::port::outbound::Runner;
use core_agent::macos_workspace::Sandbox;

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
}

impl Runner for AgentSandbox {
    fn run(&self, cmd: &str) -> Result<String, String> {
        self.inner.run(cmd).map_err(|e| e.to_string())
    }
}
