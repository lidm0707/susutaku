//! Serde types for the `rate_limits` snapshot codex writes into rollout files.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct UsageWindow {
    #[serde(default)]
    pub used_percent: f64,
    #[serde(default)]
    pub window_minutes: i64,
    #[serde(default)]
    pub resets_at: i64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RateLimits {
    #[serde(default)]
    pub primary: Option<UsageWindow>,
    #[serde(default)]
    pub secondary: Option<UsageWindow>,
    #[serde(default)]
    pub plan_type: Option<String>,
}
