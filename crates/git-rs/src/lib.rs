//! Git control over a work tree via git2: commit all changes, patch text,
//! dirty state. Host-side only — the agent sandbox never talks to git remotes.

pub mod commit;
pub mod diff;
pub mod error;
pub mod repo;

pub use diff::TaskPatch;
pub use error::GitError;
pub use repo::GitRepo;

pub const AGENT_SIGNATURE_NAME: &str = "susutaku-agent";
pub const AGENT_SIGNATURE_EMAIL: &str = "agent@susutaku.local";
pub const STAGE_PATHSPEC: &str = "*";
pub const DIFF_CONTEXT_LINES: u32 = 3;
pub const TASK_COMMIT_PREFIX: &str = "agent task: ";
