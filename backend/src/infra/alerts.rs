//! Discord webhook alerts on agent run finish. The webhook URL is
//! write-only: stored in `setting.json`, never logged or returned.

use std::time::Duration;

use super::zai_settings::{SettingsState, read_doc};

pub const FIELD_WEBHOOK_URL: &str = "alert_discord_webhook_url";
pub const WEBHOOK_TIMEOUT_SECS: u64 = 5;

pub fn read_webhook() -> Option<String> {
    read_doc()
        .get(FIELD_WEBHOOK_URL)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(str::to_owned)
}

/// Fire-and-forget POST of the run result to the configured webhook.
/// Failures are swallowed — alerts must never break the run path.
pub fn notify_agent_finished(settings: &SettingsState, agent: &str, status: &str) {
    let Some(url) = settings.alert_webhook() else {
        return;
    };
    let body =
        serde_json::json!({ "content": format!("agent {agent} finished: {status}") }).to_string();
    let res = ureq::post(&url)
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(WEBHOOK_TIMEOUT_SECS))
        .send_string(&body);
    if res.is_err() {
        tracing::warn!("discord alert delivery failed");
    }
}
