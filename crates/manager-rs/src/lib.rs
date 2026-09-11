//! Manager process library: one sandboxed work tree per agent.

pub mod manager_process;

pub use manager_process::{
    AGENTS_ROOT, AgentInfo, AgentLogs, AgentRunState, ManagerProcess, TaskOutcome,
};
