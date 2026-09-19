//! Per-agent configuration (model, persona, prompt, output, allowed tools).
//! Pure model — the SQL lives in the backend's postgres adapter.

use serde::{Deserialize, Serialize};

/// Custom reasoning depth for an agent, resolved at chat time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThinkLevel {
    #[default]
    Off,
    Low,
    Medium,
    High,
}

impl ThinkLevel {
    pub const ALL: [ThinkLevel; 4] = [
        ThinkLevel::Off,
        ThinkLevel::Low,
        ThinkLevel::Medium,
        ThinkLevel::High,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ThinkLevel::Off => "off",
            ThinkLevel::Low => "low",
            ThinkLevel::Medium => "medium",
            ThinkLevel::High => "high",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Some(ThinkLevel::Off),
            "low" => Some(ThinkLevel::Low),
            "medium" => Some(ThinkLevel::Medium),
            "high" => Some(ThinkLevel::High),
            _ => None,
        }
    }

    pub fn on(self) -> bool {
        self != ThinkLevel::Off
    }
}

/// What happens on the next chat send when the context budget is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CtxPolicy {
    /// Zed-style: summarize the thread, then continue with fresh context.
    #[default]
    Compact,
    Warn,
    NewThread,
    KeepGoing,
}

impl CtxPolicy {
    pub const ALL: [CtxPolicy; 4] = [
        CtxPolicy::Compact,
        CtxPolicy::Warn,
        CtxPolicy::NewThread,
        CtxPolicy::KeepGoing,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            CtxPolicy::Compact => "compact",
            CtxPolicy::Warn => "warn",
            CtxPolicy::NewThread => "new_thread",
            CtxPolicy::KeepGoing => "keep_going",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "compact" => Some(CtxPolicy::Compact),
            "warn" => Some(CtxPolicy::Warn),
            "new_thread" => Some(CtxPolicy::NewThread),
            "keep_going" => Some(CtxPolicy::KeepGoing),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentConfigRow {
    pub id: i64,
    pub name: String,
    pub model: String,
    pub persona: String,
    pub prompt: String,
    pub output: String,
    pub allowed_tools: Vec<String>,
    /// False = the agent must never receive images (screenshots, attachments).
    pub receive_images: bool,
    /// Reasoning depth (ThinkLevel::as_str); "off" = no thinking block.
    pub thinking: String,
    /// Context-window budget in tokens (model APIs don't report it).
    pub ctx_limit: i64,
    /// Full-context behaviour (CtxPolicy::as_str).
    pub ctx_policy: String,
}

impl AgentConfigRow {
    pub fn think_level(&self) -> ThinkLevel {
        ThinkLevel::parse(&self.thinking).unwrap_or_default()
    }
}

pub struct AgentConfigUpdate<'a> {
    pub id: i64,
    pub name: &'a str,
    pub model: &'a str,
    pub persona: &'a str,
    pub prompt: &'a str,
    pub output: &'a str,
    pub allowed_tools: &'a [String],
    pub receive_images: bool,
    pub thinking: &'a str,
    pub ctx_limit: i64,
    pub ctx_policy: &'a str,
}
