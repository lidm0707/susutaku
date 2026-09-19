//! Manager-backed git adapter: routes git toolcalls for a named agent.
//! Clone/status/diff run host-side in its work tree; branch/commit/push/pr
//! run inside the agent's own podman container (see manager-rs git_in_sandbox).

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

    pub(crate) fn to_proto(op: &GitOp) -> Result<proto_rs::GitTool, String> {
        match op {
            GitOp::Clone { url, token } => Ok(proto_rs::GitTool::Clone {
                url: url.clone().ok_or_else(|| "clone needs a url".to_string())?,
                token: token.clone(),
            }),
            GitOp::Status => Ok(proto_rs::GitTool::Status),
            GitOp::Diff => Ok(proto_rs::GitTool::Diff),
            GitOp::Branch { name } => Ok(proto_rs::GitTool::Branch { name: name.clone() }),
            GitOp::TaskBranch { name } => Ok(proto_rs::GitTool::TaskBranch { name: name.clone() }),
            GitOp::Commit { message } => Ok(proto_rs::GitTool::Commit {
                message: message.clone(),
            }),
            GitOp::Push { branch, url, token } => Ok(proto_rs::GitTool::Push {
                branch: branch.clone(),
                url: url.clone(),
                token: token.clone(),
            }),
            GitOp::PullRequest {
                title,
                head,
                base,
                url,
                token,
            } => Ok(proto_rs::GitTool::PullRequest {
                title: title.clone(),
                head: head.clone(),
                base: base.clone(),
                url: url.clone(),
                token: token.clone(),
            }),
        }
    }
}

impl AgentGit for ManagerGit {
    fn git_for(&self, agent: &str, op: &GitOp) -> Result<String, String> {
        self.manager.git_tool(agent, &Self::to_proto(op)?)
    }
}
