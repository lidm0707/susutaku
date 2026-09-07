//! Client workspace environment reported by the browser client and
//! persisted in the repo-root `setting.json` under the `client_env`
//! section so the backend can share the machine's workspace details.

use serde::{Deserialize, Serialize};

pub const CLIENT_ENV_SECTION: &str = "client_env";
pub const FIELD_USER_AGENT: &str = "user_agent";
pub const FIELD_PLATFORM: &str = "platform";
pub const FIELD_LANGUAGE: &str = "language";
pub const FIELD_TIMEZONE: &str = "timezone";
pub const FIELD_SCREEN: &str = "screen";
pub const FIELD_WORKSPACE_PATH: &str = "workspace_path";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClientEnv {
    pub user_agent: String,
    pub platform: String,
    pub language: String,
    pub timezone: String,
    pub screen: String,
    pub workspace_path: String,
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
    });
}

fn str_field(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
