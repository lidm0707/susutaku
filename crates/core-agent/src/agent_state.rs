use std::sync::RwLock;

use crate::toolcall::web_search::SearchResult;

pub const MAX_HISTORY: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Thinking,
    Acting,
    Done,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    User,
    Agent,
    Tool,
}

#[derive(Debug)]
pub struct AgentState {
    phase: RwLock<Phase>,
    history: RwLock<Vec<Message>>,
}

impl AgentState {
    pub fn new() -> Self {
        Self {
            phase: RwLock::new(Phase::Idle),
            history: RwLock::new(Vec::new()),
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase.read().map(|p| p.clone()).unwrap_or(Phase::Idle)
    }

    pub fn set_phase(&self, phase: Phase) {
        if let Ok(mut p) = self.phase.write() {
            *p = phase;
        }
    }

    pub fn push(&self, role: Role, content: impl Into<String>) {
        if let Ok(mut h) = self.history.write() {
            h.push(Message {
                role,
                content: content.into(),
            });
            let excess = h.len().saturating_sub(MAX_HISTORY);
            h.drain(..excess);
        }
    }

    pub fn transcript(&self) -> Vec<Message> {
        self.history.read().map(|h| h.clone()).unwrap_or_default()
    }

    /// Record one tool execution in the transcript and return its summary.
    /// The generic half of the agent loop: any tool the harness dispatches
    /// lands here (search, fetch, lsp — see `toolcall::Tool`).
    pub fn apply_tool(&self, tool_name: &str, query: &str, output: &str) -> String {
        self.set_phase(Phase::Acting);
        self.push(Role::User, format!("[{tool_name}] {query}"));
        self.push(Role::Tool, output.to_string());
        self.set_phase(Phase::Done);
        output.to_string()
    }

    pub fn apply_search(&self, query: &str, results: &[SearchResult]) -> String {
        let summary = SearchResult::summarize(results);
        self.apply_tool("web_search", query, &summary)
    }
}

impl Default for AgentState {
    fn default() -> Self {
        Self::new()
    }
}
