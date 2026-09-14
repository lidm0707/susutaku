//! Port: runs shell commands inside the agent sandbox.

use crate::domain::GitOp;

pub trait Runner: Send + Sync + 'static {
    fn run(&self, cmd: &str) -> Result<String, String>;
    /// Write a whole file into the agent work tree (coding tool). `path` is
    /// workspace-relative; escaping the work tree is rejected.
    fn write_file(&self, path: &str, content: &str) -> Result<(), String>;
    /// Read a whole file from the agent work tree (lsp tool). `path` is
    /// workspace-relative; escaping the work tree is rejected.
    fn read_file(&self, path: &str) -> Result<String, String>;
    /// Absolute host path of the agent work tree (lsp tool resolves it).
    fn workspace_root(&self) -> std::path::PathBuf;
    /// Whether the work tree holds a git repo (coding tool fits).
    fn has_git_repo(&self) -> bool;
    /// Host-side git operation against the agent work tree (clone needs
    /// network, so this never runs inside the sandbox).
    fn git(&self, op: &GitOp) -> Result<String, String>;
}
