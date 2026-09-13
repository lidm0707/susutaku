//! Client workspace environment reported by the browser client and
//! detected server-side from the installed machine (never trusted from the
//! browser) and persisted in the repo-root `setting.json` under the
//! `client_env` section.

use serde::{Deserialize, Serialize};

pub const CLIENT_ENV_SECTION: &str = "client_env";
pub const FIELD_USER_AGENT: &str = "user_agent";
pub const FIELD_PLATFORM: &str = "platform";
pub const FIELD_LANGUAGE: &str = "language";
pub const FIELD_TIMEZONE: &str = "timezone";
pub const FIELD_SCREEN: &str = "screen";
pub const FIELD_WORKSPACE_PATH: &str = "workspace_path";
pub const FIELD_HOSTNAME: &str = "hostname";
pub const FIELD_OS: &str = "os";
pub const FIELD_ARCH: &str = "arch";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClientEnv {
    pub user_agent: String,
    pub platform: String,
    pub language: String,
    pub timezone: String,
    pub screen: String,
    pub workspace_path: String,
    pub hostname: String,
    pub os: String,
    pub arch: String,
}

/// The backend is installed on the target machine, so the authoritative
/// host identity comes from here, never from the browser.
pub fn host_fingerprint() -> (String, String, String) {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_default();
    (
        hostname,
        std::env::consts::OS.to_owned(),
        std::env::consts::ARCH.to_owned(),
    )
}

pub fn read(doc: &serde_json::Value) -> Option<ClientEnv> {
    let v = doc.get(CLIENT_ENV_SECTION)?;
    Some(ClientEnv {
        user_agent: str_field(v, FIELD_USER_AGENT),
        platform: str_field(v, FIELD_PLATFORM),
        language: str_field(v, FIELD_LANGUAGE),
        timezone: str_field(v, FIELD_TIMEZONE),
        screen: str_field(v, FIELD_SCREEN),
        workspace_path: str_field(v, FIELD_WORKSPACE_PATH),
        hostname: str_field(v, FIELD_HOSTNAME),
        os: str_field(v, FIELD_OS),
        arch: str_field(v, FIELD_ARCH),
    })
}

pub fn write(doc: &mut serde_json::Value, env: &ClientEnv) {
    doc[CLIENT_ENV_SECTION] = serde_json::json!({
        FIELD_USER_AGENT: env.user_agent,
        FIELD_PLATFORM: env.platform,
        FIELD_LANGUAGE: env.language,
        FIELD_TIMEZONE: env.timezone,
        FIELD_SCREEN: env.screen,
        FIELD_WORKSPACE_PATH: env.workspace_path,
        FIELD_HOSTNAME: env.hostname,
        FIELD_OS: env.os,
        FIELD_ARCH: env.arch,
    });
}

fn str_field(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
