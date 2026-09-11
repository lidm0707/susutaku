//! Z.ai settings persisted in the repo-root `setting.json`.
//! API keys are write-only from the API's point of view: they are stored to
//! disk and handed to `ZaiClient`, never echoed back to the frontend.

use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use zai_api::client::{DEFAULT_MODEL, ENV_API_KEY};

use super::client_env::{self, ClientEnv};
use super::local_settings;

pub const SETTINGS_FILE: &str = "setting.json";
/// Override for containers: the deploy stack points this at a volume so
/// settings survive redeploys.
pub const SETTINGS_FILE_ENV: &str = "SUSUTAKU_SETTINGS";
pub const ZAI_SECTION: &str = "zai";
pub const SYSTEM_SECTION: &str = "system";
pub const FIELD_API_KEY: &str = "api_key";
pub const FIELD_MODEL: &str = "model";
pub const FIELD_MODELS: &str = "models";
pub const FIELD_PROMPT: &str = "prompt";
pub const FIELD_SAY_HI_TIME: &str = "say_hi_time";
pub const FIELD_SAY_HI_INTERVAL: &str = "say_hi_interval_mins";
pub const FIELD_TIMEZONE: &str = "timezone";
pub const TIME_LEN: usize = 5;
/// Repeat interval for say hi, in minutes (1 step every N minutes).
pub const MIN_SAY_HI_INTERVAL: u64 = 1;
pub const MAX_SAY_HI_INTERVAL: u64 = 24 * 60;

/// Validates `HH:MM` (24h). Returns hour/minute for scheduler use.
pub fn parse_hhmm(value: &str) -> Result<(u32, u32), String> {
    const HOURS: u32 = 24;
    const MINUTES: u32 = 60;
    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() != 2 {
        return Err(format!("invalid time {value:?}: expected HH:MM"));
    }
    let parse = |p: &str| -> Result<u32, String> {
        if p.len() != 2 || !p.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("invalid time {value:?}: expected HH:MM"));
        }
        p.parse::<u32>().map_err(|_| format!("invalid time {value:?}"))
    };
    let (h, m) = (parse(parts[0])?, parse(parts[1])?);
    if h >= HOURS || m >= MINUTES {
        return Err(format!("invalid time {value:?}: out of range"));
    }
    Ok((h, m))
}

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
    #[serde(default)]
    pub say_hi_time: Option<String>,
    /// When set, say hi repeats every N minutes after the start time.
    #[serde(default)]
    pub say_hi_interval_mins: Option<u64>,
    #[serde(default)]
    pub timezone: Option<String>,
}

pub fn validate_api_key(raw_key: &str) -> Result<(), String> {
    const MAX_KEY_LEN: usize = 256;
    let key = zai_api::client::strip_bearer_scheme(raw_key);
    if key.chars().any(char::is_whitespace) {
        return Err("api key must not contain whitespace or newlines".into());
    }
    if key.len() > MAX_KEY_LEN {
        return Err(format!("api key longer than {MAX_KEY_LEN} bytes"));
    }
    Ok(())
}

pub struct SettingsState {
    zai: RwLock<ZaiSettings>,
    client_env: RwLock<Option<ClientEnv>>,
    system_prompt: RwLock<String>,
    local_endpoint: RwLock<String>,
    alert_webhook: RwLock<Option<String>>,
}

impl SettingsState {
    pub fn load() -> Self {
        let doc = read_doc();
        Self {
            zai: RwLock::new(read_file().unwrap_or_default()),
            client_env: RwLock::new(client_env::read(&doc)),
            system_prompt: RwLock::new(read_system_prompt()),
            local_endpoint: RwLock::new(
                local_settings::read(&doc)
                    .map(|s| s.endpoint)
                    .unwrap_or_default(),
            ),
            alert_webhook: RwLock::new(super::alerts::read_webhook()),
        }
    }

    /// Webhook URL is write-only: it never leaves the backend.
    pub fn alert_webhook(&self) -> Option<String> {
        self.alert_webhook
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn set_alert_webhook(&self, url: Option<&str>) -> Result<(), String> {
        let mut doc = read_doc();
        match url.map(str::trim).filter(|u| !u.is_empty()) {
            Some(u) => doc[super::alerts::FIELD_WEBHOOK_URL] = serde_json::json!(u),
            None => {
                doc.as_object_mut()
                    .ok_or_else(|| "settings doc is not an object".to_owned())?
                    .remove(super::alerts::FIELD_WEBHOOK_URL);
            }
        }
        write_doc(&doc)?;
        *self
            .alert_webhook
            .write()
            .unwrap_or_else(|e| e.into_inner()) = super::alerts::read_webhook();
        Ok(())
    }

    pub fn local_endpoint(&self) -> String {
        self.local_endpoint
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn set_local_endpoint(&self, url: &str) -> Result<(), String> {
        let mut doc = read_doc();
        local_settings::write(&mut doc, url);
        write_doc(&doc)?;
        *self
            .local_endpoint
            .write()
            .unwrap_or_else(|e| e.into_inner()) = url.to_owned();
        Ok(())
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
            validate_api_key(&k)?;
            zai.api_key = Some(k.to_owned());
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
        validate_api_key(api_key.trim())?;
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
        validate_api_key(api_key.trim())?;
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

    pub fn set_zai_schedule(
        &self,
        say_hi_time: Option<String>,
        say_hi_interval: Option<u64>,
        timezone: Option<String>,
    ) -> Result<(), String> {
        let time = say_hi_time.filter(|t| !t.is_empty());
        if let Some(t) = &time {
            parse_hhmm(t)?;
        }
        let interval = say_hi_interval
            .filter(|v| *v >= MIN_SAY_HI_INTERVAL)
            .map(|v| v.clamp(MIN_SAY_HI_INTERVAL, MAX_SAY_HI_INTERVAL));
        let tz = timezone.as_deref().map(str::trim).filter(|t| !t.is_empty());
        if let Some(t) = tz {
            t.parse::<chrono_tz::Tz>()
                .map_err(|_| format!("unknown timezone {t:?}"))?;
        }
        let mut zai = self.zai.write().unwrap_or_else(|e| e.into_inner());
        zai.say_hi_time = time;
        zai.say_hi_interval_mins = interval;
        zai.timezone = tz.map(str::to_owned);
        write_file(&zai)
    }

    /// Token priority: the active model's own key, the saved global key, `$ZAI_API_KEY`.
    pub fn zai_token(&self) -> Result<String, String> {
        let zai = self.zai();
        let own_key = zai
            .models
            .iter()
            .find(|m| m.model == zai.model.as_deref().unwrap_or(DEFAULT_MODEL))
            .and_then(ZaiModel::key);
        match own_key.or(zai.api_key.as_deref()) {
            Some(k) => Ok(zai_api::client::strip_bearer_scheme(k).to_string()),
            None => std::env::var(zai_api::client::ENV_API_KEY)
                .ok()
                .map(|k| k.trim().to_owned())
                .filter(|k| !k.is_empty())
                .ok_or_else(|| format!("no Z.ai key: save one in settings or set {ENV_API_KEY}")),
        }
    }

    pub fn zai_client(&self) -> Result<zai_api::client::ZaiClient, String> {
        let zai = self.zai();
        let name = zai
            .model
            .clone()
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let token = self.zai_token()?;
        Ok(zai_api::client::ZaiClient::from_key(&token, &name))
    }
}

pub(crate) fn read_doc() -> serde_json::Value {
    std::fs::read(settings_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

fn settings_path() -> String {
    std::env::var(SETTINGS_FILE_ENV).unwrap_or_else(|_| SETTINGS_FILE.to_owned())
}

pub(crate) fn write_doc(doc: &serde_json::Value) -> Result<(), String> {
    std::fs::write(
        settings_path(),
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
        say_hi_time: zai
            .get(FIELD_SAY_HI_TIME)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        say_hi_interval_mins: zai
            .get(FIELD_SAY_HI_INTERVAL)
            .and_then(serde_json::Value::as_u64),
        timezone: zai
            .get(FIELD_TIMEZONE)
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
        FIELD_SAY_HI_TIME: zai.say_hi_time,
        FIELD_SAY_HI_INTERVAL: zai.say_hi_interval_mins,
        FIELD_TIMEZONE: zai.timezone,
    });
    write_doc(&doc)
}
