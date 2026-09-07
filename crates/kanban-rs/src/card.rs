use serde::{Deserialize, Serialize};

pub type CardId = u64;

pub const MIN_PRIORITY: u8 = 0;
pub const MAX_PRIORITY: u8 = 9;
pub const DEFAULT_PRIORITY: u8 = 5;

pub const PRIORITY_LOW: &str = "low";
pub const PRIORITY_NORMAL: &str = "normal";
pub const PRIORITY_HIGH: &str = "high";
pub const PRIORITY_CRITICAL: &str = "critical";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

impl Priority {
    pub fn level(self) -> u8 {
        match self {
            Priority::Low => 1,
            Priority::Normal => DEFAULT_PRIORITY,
            Priority::High => 8,
            Priority::Critical => MAX_PRIORITY,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    pub id: CardId,
    pub title: String,
    pub description: String,
    pub priority: Priority,
}

impl Card {
    pub fn new(id: CardId, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            description: String::new(),
            priority: Priority::Normal,
        }
    }
}
