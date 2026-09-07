//! Z.ai settings persisted in the repo-root `setting.json`.
//! The API key is write-only from the API's point of view: it is stored to
//! disk and handed to `ZaiClient`, never echoed back to the frontend.

use std::sync::RwLock;

use serde::Serialize;
use zai_api::client::{DEFAULT_MODEL, ENV_API_KEY};

pub const SETTINGS_FILE: &str = "setting.json";
pub const ZAI_SECTION: &str = "zai";
pub const FIELD_API_KEY: &str = "api_key";
pub const FIELD_MODEL: &str = "model";

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ZaiSettings {
    pub model: Option<String>,
    #[serde(skip_serializing)]
    pub api_key: Option<String>,
}

pub struct SettingsState {
    zai: RwLock<ZaiSettings>,
}

impl SettingsState {
    pub fn load() -> Self {
        Self {
            zai: RwLock::new(read_file().unwrap_or_default()),
        }
    }

    pub fn zai(&self) -> ZaiSettings {
        self.zai.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_zai(&self, api_key: Option<String>, model: Option<String>) -> Result<(), String> {
        let mut zai = self.zai.write().unwrap_or_else(|e| e.into_inner());
        if let Some(k) = api_key.filter(|k| !k.is_empty()) {
            zai.api_key = Some(k);
        }
        if let Some(m) = model.filter(|m| !m.is_empty()) {
            zai.model = Some(m);
        }
        write_file(&zai)
    }

    /// Key priority: saved setting, then `$ZAI_API_KEY`.
    pub fn zai_client(&self) -> Result<zai_api::client::ZaiClient, String> {
        let zai = self.zai();
        let model = zai
            .model
            .clone()
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let client = match zai.api_key {
            Some(key) => zai_api::client::ZaiClient::from_key(&key, &model),
            None => zai_api::client::ZaiClient::from_env()
                .map_err(|_| format!("no Z.ai key: save one in settings or set {ENV_API_KEY}"))?,
        };
        Ok(client)
    }
}

fn read_file() -> Option<ZaiSettings> {
    let bytes = std::fs::read(SETTINGS_FILE).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let zai = v.get(ZAI_SECTION)?.clone();
    Some(ZaiSettings {
        model: zai
            .get(FIELD_MODEL)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        api_key: zai
            .get(FIELD_API_KEY)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
    })
}

fn write_file(zai: &ZaiSettings) -> Result<(), String> {
    let mut doc = match std::fs::read(SETTINGS_FILE) {
        Ok(bytes) => serde_json::from_slice::<serde_json::Value>(&bytes)
            .unwrap_or_else(|_| serde_json::json!({})),
        Err(_) => serde_json::json!({}),
    };
    doc[ZAI_SECTION] = serde_json::json!({
        FIELD_API_KEY: zai.api_key.as_deref().unwrap_or(""),
        FIELD_MODEL: zai.model.as_deref().unwrap_or(DEFAULT_MODEL),
    });
    std::fs::write(
        SETTINGS_FILE,
        serde_json::to_vec_pretty(&doc).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("write {SETTINGS_FILE}: {e}"))
}
