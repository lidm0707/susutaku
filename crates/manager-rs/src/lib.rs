//! Manager process library: one sandboxed work tree per agent.

pub mod manager_process;

pub use manager_process::{AgentInfo, AgentRunState, ManagerProcess, TaskOutcome, AGENTS_ROOT};
