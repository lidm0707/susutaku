//! Manager-backed git adapter: runs git toolcalls host-side in a named
//! agent's own work tree (credentials live here; the sandbox has no network).

use crate::domain::GitOp;
use crate::port::outbound::AgentGit;
use manager_rs::manager::Manager;
use std::sync::Arc;

pub struct ManagerGit {
    manager: Arc<Manager>,
}

impl ManagerGit {
    pub fn new(manager: Arc<Manager>) -> Self {
        Self { manager }
    }

    fn to_proto(op: &GitOp) -> Result<proto_rs::GitTool, String> {
        match op {
            GitOp::Clone { url, token } => Ok(proto_rs::GitTool::Clone {
                url: url.clone().ok_or_else(|| "clone needs a url".to_string())?,
                token: token.clone(),
            }),
            GitOp::Status => Ok(proto_rs::GitTool::Status),
            GitOp::Diff => Ok(proto_rs::GitTool::Diff),
        }
    }
}

impl AgentGit for ManagerGit {
    fn git_for(&self, agent: &str, op: &GitOp) -> Result<String, String> {
        self.manager.git_tool(agent, &Self::to_proto(op)?)
    }
}
