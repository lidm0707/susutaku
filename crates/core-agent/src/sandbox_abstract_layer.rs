//! `sandbox_abstract_layer` — the platform-agnostic contract every sandbox
//! backend must satisfy, plus an honest classification of what each backend
//! actually guarantees.
//!
//! The backend (`backend/`, agent runners, tool executors) must depend on this
//! trait, never on a concrete platform module, so swapping the isolation
//! technology (Linux namespaces, Windows restricted tokens, macOS sandbox-exec,
//! …) does not change call sites.
//!
//! # Guarantee levels
//!
//! [`Guarantee`] describes how much a given implementation enforces at the
//! **kernel** level. Anything below [`Guarantee::Kernel`] must be treated as
//! *advisory*: commands run with the backend's own privileges and only
//! conventionally stay inside the workspace.

use std::io::Error;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Speaker role in a sandbox transcript (canonical definition; every platform
/// module re-exports this).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    User,
    Agent,
    Tool,
}

/// One transcript entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub role: Role,
    pub content: String,
}

/// Persisted sandbox state: logical cwd + transcript. Never contains host
/// secrets — callers must only push non-sensitive context.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SandboxState {
    pub cwd: String,
    pub history: Vec<HistoryEntry>,
}

/// How strongly a platform implementation isolates untrusted commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guarantee {
    /// Kernel-enforced isolation (namespaces / tokens + ACLs / seatbelt):
    /// the process *cannot* escape by ordinary syscalls.
    Kernel,
    /// Best-effort: some kernel primitives, but not full isolation.
    /// Commands must still be treated as untrusted-to-escape.
    BestEffort,
    /// Soft isolation only (working directory + conventions). NOT a sandbox.
    Soft,
}

/// What a sandbox implementation must expose, independent of platform.
///
/// Implementations: [`crate::sandbox::linux::Sandbox`] (Kernel),
/// [`crate::sandbox::windows::Sandbox`] (BestEffort: restricted token + job
/// object + DACL; network not hard-blocked),
/// [`crate::sandbox::macos::Sandbox`] (Soft today).
pub trait SandboxLayer: Send + Sync {
    /// Kernel-level strength of this implementation.
    fn guarantee(&self) -> Guarantee;

    /// Workspace root the sandboxed commands run in (host-side view).
    fn root(&self) -> PathBuf;

    /// Execute `cmd` in the sandbox; returns bounded stdout.
    fn run(&self, cmd: &str) -> Result<String, Error>;

    /// Append a conversation entry to the persisted transcript.
    fn push_context(&self, role: Role, content: impl Into<String>);

    /// Current transcript.
    fn transcript(&self) -> Vec<HistoryEntry>;

    /// Move the logical working directory; must stay inside `root()`.
    fn set_cwd(&self, rel: &str) -> Result<(), Error>;

    /// Full teardown: kill process tree, delete workspace and state.
    fn purge(&self);

    /// Remove leftovers of dead backend instances (startup call).
    fn purge_stale();

    /// Unique id of this sandbox instance (also in the dir marker/metadata).
    fn id(&self) -> &str;
}
