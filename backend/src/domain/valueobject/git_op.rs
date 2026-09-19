//! A git operation the agent may request. Clone/status/diff run host-side
//! against the agent work tree; branch/commit/push/pr run inside the
//! agent's own podman container (network + run-scoped token env).

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitOp {
    /// `None` url means "the repo bound to this chat's project".
    Clone {
        url: Option<String>,
        token: Option<String>,
    },
    Status,
    Diff,
    Branch {
        name: String,
    },
    /// Publish automation only: create-or-move `name` to HEAD and check it
    /// out, so the task branch holds the task commits before push/PR.
    TaskBranch {
        name: String,
    },
    Commit {
        message: String,
    },
    Push {
        branch: String,
        url: Option<String>,
        token: Option<String>,
    },
    PullRequest {
        title: String,
        /// Empty = the container's current branch.
        head: String,
        base: String,
        url: Option<String>,
        token: Option<String>,
    },
}

impl GitOp {
    /// Ops that only make sense in a named agent's own sandbox — the host
    /// no longer commits or pushes on the agent's behalf.
    pub const fn sandbox_only(&self) -> bool {
        matches!(
            self,
            GitOp::Branch { .. }
                | GitOp::Commit { .. }
                | GitOp::Push { .. }
                | GitOp::PullRequest { .. }
        )
    }
}
