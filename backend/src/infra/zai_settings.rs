//! Z.ai settings persisted in the repo-root `setting.json`.
//! API keys are write-only from the API's point of view: they are stored to
//! disk and handed to `ZaiClient`, never echoed back to the frontend.

use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use zai_api::client::{DEFAULT_MODEL, ENV_API_KEY};

use super::client_env::{self, ClientEnv};

pub const SETTINGS_FILE: &str = "setting.json";
pub const ZAI_SECTION: &str = "zai";
pub const SYSTEM_SECTION: &str = "system";
pub const FIELD_API_KEY: &str = "api_key";
pub const FIELD_MODEL: &str = "model";
pub const FIELD_MODELS: &str = "models";
pub const FIELD_PROMPT: &str = "prompt";

/// One created model with its own API token.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ZaiModel {
    pub model: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

impl ZaiModel {
    pub fn key(&self) -> Option<&str> {
        self.api_key.as_deref().filter(|k| !k.is_empty())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ZaiSettings {
    pub model: Option<String>,
    pub models: Vec<ZaiModel>,
    #[serde(skip_serializing)]
    pub api_key: Option<String>,
}

pub struct SettingsState {
    zai: RwLock<ZaiSettings>,
    client_env: RwLock<Option<ClientEnv>>,
    system_prompt: RwLock<String>,
}

impl SettingsState {
    pub fn load() -> Self {
        let doc = read_doc();
        Self {
            zai: RwLock::new(read_file().unwrap_or_default()),
            client_env: RwLock::new(client_env::read(&doc)),
            system_prompt: RwLock::new(read_system_prompt()),
        }
    }

    pub fn client_env(&self) -> Option<ClientEnv> {
        self.client_env
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn set_client_env(&self, env: &ClientEnv) -> Result<(), String> {
        let mut doc = read_doc();
        client_env::write(&mut doc, env);
        write_doc(&doc)?;
        *self.client_env.write().unwrap_or_else(|e| e.into_inner()) = Some(env.clone());
        Ok(())
    }

    pub fn system_prompt(&self) -> String {
        self.system_prompt
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn set_system_prompt(&self, prompt: &str) -> Result<(), String> {
        let mut doc = read_doc();
        doc[SYSTEM_SECTION] = serde_json::json!({ FIELD_PROMPT: prompt });
        write_doc(&doc)?;
        *self
            .system_prompt
            .write()
            .unwrap_or_else(|e| e.into_inner()) = prompt.to_owned();
        Ok(())
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

    pub fn zai_add_model(&self, model: &str, api_key: &str) -> Result<(), String> {
        let name = model.trim();
        if name.is_empty() {
            return Err("model name is empty".into());
        }
        if api_key.trim().is_empty() {
            return Err("api key is required for a new model".into());
        }
        let mut zai = self.zai.write().unwrap_or_else(|e| e.into_inner());
        if zai.models.iter().any(|m| m.model == name) {
            return Err(format!("{name} already exists"));
        }
        zai.models.push(ZaiModel {
            model: name.to_owned(),
            api_key: Some(api_key.trim().to_owned()),
        });
        if zai.model.is_none() {
            zai.model = Some(name.to_owned());
        }
        write_file(&zai)
    }

    pub fn zai_set_key(&self, model: &str, api_key: &str) -> Result<(), String> {
        if api_key.trim().is_empty() {
            return Err("api key is empty".into());
        }
        let mut zai = self.zai.write().unwrap_or_else(|e| e.into_inner());
        let entry = zai
            .models
            .iter_mut()
            .find(|m| m.model == model)
            .ok_or_else(|| format!("{model} not found"))?;
        entry.api_key = Some(api_key.trim().to_owned());
        write_file(&zai)
    }

    pub fn zai_remove_model(&self, model: &str) -> Result<(), String> {
        let mut zai = self.zai.write().unwrap_or_else(|e| e.into_inner());
        let before = zai.models.len();
        zai.models.retain(|m| m.model != model);
        if zai.models.len() == before {
            return Err(format!("{model} not found"));
        }
        if zai.model.as_deref() == Some(model) {
            zai.model = zai.models.first().map(|m| m.model.clone());
        }
        write_file(&zai)
    }

    pub fn zai_set_active(&self, model: &str) -> Result<(), String> {
        let mut zai = self.zai.write().unwrap_or_else(|e| e.into_inner());
        if !zai.models.iter().any(|m| m.model == model) {
            return Err(format!("{model} not found"));
        }
        zai.model = Some(model.to_owned());
        write_file(&zai)
    }

    /// Key priority: the active model's own key, the saved global key, `$ZAI_API_KEY`.
    pub fn zai_client(&self) -> Result<zai_api::client::ZaiClient, String> {
        let zai = self.zai();
        let name = zai
            .model
            .clone()
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let own_key = zai
            .models
            .iter()
            .find(|m| m.model == name)
            .and_then(ZaiModel::key);
        let key = own_key.or(zai.api_key.as_deref());
        let client = match key {
            Some(k) => zai_api::client::ZaiClient::from_key(k, &name),
            None => zai_api::client::ZaiClient::from_env()
                .map_err(|_| format!("no Z.ai key: save one in settings or set {ENV_API_KEY}"))?,
        };
        Ok(client)
    }
}

fn read_doc() -> serde_json::Value {
    std::fs::read(SETTINGS_FILE)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

fn write_doc(doc: &serde_json::Value) -> Result<(), String> {
    std::fs::write(
        SETTINGS_FILE,
        serde_json::to_vec_pretty(doc).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("write {SETTINGS_FILE}: {e}"))
}

fn read_file() -> Option<ZaiSettings> {
    let zai = read_doc().get(ZAI_SECTION)?.clone();
    let model = zai
        .get(FIELD_MODEL)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let mut models: Vec<ZaiModel> = zai
        .get(FIELD_MODELS)
        .and_then(|v| serde_json::from_value::<Vec<ZaiModel>>(v.clone()).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|m| !m.model.is_empty())
        .collect();
    if let Some(name) = model
        .as_deref()
        .filter(|name| !models.iter().any(|m| &m.model == name))
    {
        models.insert(
            0,
            ZaiModel {
                model: name.to_owned(),
                api_key: None,
            },
        );
    }
    Some(ZaiSettings {
        model,
        models,
        api_key: zai
            .get(FIELD_API_KEY)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
    })
}

fn read_system_prompt() -> String {
    read_doc()
        .get(SYSTEM_SECTION)
        .and_then(|s| s.get(FIELD_PROMPT))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn write_file(zai: &ZaiSettings) -> Result<(), String> {
    let mut doc = read_doc();
    doc[ZAI_SECTION] = serde_json::json!({
        FIELD_API_KEY: zai.api_key.as_deref().unwrap_or(""),
        FIELD_MODEL: zai.model.as_deref().unwrap_or(DEFAULT_MODEL),
        FIELD_MODELS: zai.models,
    });
    write_doc(&doc)
}
