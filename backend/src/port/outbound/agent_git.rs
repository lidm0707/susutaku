//! Port: host-side git operations against a named agent's own work tree.

use crate::domain::GitOp;

pub trait AgentGit: Send + Sync + 'static {
    /// Runs `op` in `agent`'s work tree (spawning the agent on demand).
    /// Falls back to the shared work tree only when no agent is named.
    fn git_for(&self, agent: &str, op: &GitOp) -> Result<String, String>;
}
