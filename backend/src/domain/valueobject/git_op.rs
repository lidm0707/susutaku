//! A git operation the agent may request: clone/status/diff, always executed
//! host-side against the agent work tree (the sandbox has no network).

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitOp {
    Clone { url: String, token: Option<String> },
    Status,
    Diff,
}
