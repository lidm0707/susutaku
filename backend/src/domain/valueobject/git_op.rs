//! A git operation the agent may request: clone/status/diff, always executed
//! host-side against the agent work tree (the sandbox has no network).

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitOp {
    /// `None` url means "the repo bound to this chat's project".
    Clone {
        url: Option<String>,
        token: Option<String>,
    },
    Status,
    Diff,
}
