//! Agent-run port: run a command inside a named agent's own sandbox.

pub trait AgentRun: Send + Sync + 'static {
    /// Runs `cmd` as `agent` inside its sandbox (spawning it on demand).
    /// Output becomes the agent's pending result; the command joins its
    /// transcript and the slot's run counter advances.
    fn run_for(&self, agent: &str, cmd: &str) -> Result<String, String>;
}
