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
}
