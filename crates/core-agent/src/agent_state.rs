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

    pub fn apply_search(&self, query: &str, results: &[SearchResult]) -> String {
        self.set_phase(Phase::Acting);
        self.push(Role::User, query);
        let summary = SearchResult::summarize(results);
        self.push(Role::Tool, summary.clone());
        self.set_phase(Phase::Done);
        summary
    }
}

impl Default for AgentState {
    fn default() -> Self {
        Self::new()
    }
}
