//! Remote provider client: local-model HTTP adapter speaking the
//! OpenAI-compatible Chat Completions protocol first, with the legacy
//! proto as fallback.

use std::sync::RwLock;

use susutaku_mlx::stats::GenStats;
use susutaku_mlx::tok::TokKind;

use crate::port::outbound::{GenReply, Inference, ModelEndpoint, ModelSwitch, ReplyRx};

const HTTP_OK_RANGE: std::ops::Range<u16> = 200..300;
const MODELS_PATH: &str = "/api/models";
const SELECT_PATH: &str = "/api/models/select";
const INFERENCE_PATH: &str = "/api/inference";
const OPENAI_MODELS_PATH: &str = "/v1/models";
const OPENAI_CHAT_PATH: &str = "/v1/chat/completions";
const OPENAI_ENGINE: &str = "openai";
const OPENAI_DEFAULT_MODEL: &str = "default";
const ZERO_SECS: f64 = 0.0;

pub struct ModelEntry {
    pub name: String,
    pub loadable: bool,
    pub bytes: u64,
    pub engine: String,
    pub selected: bool,
}

/// Catalog of models hosted on the remote model server.
pub trait ModelCatalog: Send + Sync + 'static {
    fn list(&self) -> Result<Vec<ModelEntry>, String>;
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Auto,
    OpenAi,
    Legacy,
}

pub struct RemoteModel {
    base_url: RwLock<String>,
    selected: RwLock<Option<String>>,
    mode: RwLock<Mode>,
}

impl RemoteModel {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: RwLock::new(trim_url(base_url)),
            selected: RwLock::new(None),
            mode: RwLock::new(Mode::Auto),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url())
    }

    fn base_url(&self) -> String {
        self.base_url
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn selected(&self) -> Option<String> {
        self.selected
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn set_selected(&self, name: Option<String>) {
        *self.selected.write().unwrap_or_else(|e| e.into_inner()) = name;
    }

    fn reset(&self, url: String) {
        *self.base_url.write().unwrap_or_else(|e| e.into_inner()) = url;
        *self.mode.write().unwrap_or_else(|e| e.into_inner()) = Mode::Auto;
        *self.selected.write().unwrap_or_else(|e| e.into_inner()) = None;
    }

    fn mode(&self) -> Mode {
        let mut mode = self.mode.write().unwrap_or_else(|e| e.into_inner());
        if *mode == Mode::Auto {
            *mode = detect_mode(&self.base_url());
        }
        *mode
    }

    fn post_json(url: String, body: serde_json::Value) -> Result<serde_json::Value, String> {
        let resp = ureq::post(&url)
            .send_json(body)
            .map_err(|e| format!("POST {url}: {e}"))?;
        to_value(resp, &url)
    }

    fn get_json(url: String) -> Result<serde_json::Value, String> {
        let resp = ureq::get(&url)
            .call()
            .map_err(|e| format!("GET {url}: {e}"))?;
        to_value(resp, &url)
    }
}

impl ModelEndpoint for RemoteModel {
    fn set_base_url(&self, url: &str) {
        self.reset(trim_url(url));
    }

    fn base_url(&self) -> String {
        RemoteModel::base_url(self)
    }
}

fn trim_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

fn detect_mode(base: &str) -> Mode {
    match RemoteModel::get_json(format!("{base}{OPENAI_MODELS_PATH}")) {
        Ok(v) if v.get("data").and_then(|d| d.as_array()).is_some() => Mode::OpenAi,
        _ => Mode::Legacy,
    }
}

fn to_value(resp: ureq::Response, url: &str) -> Result<serde_json::Value, String> {
    if !HTTP_OK_RANGE.contains(&resp.status()) {
        return Err(format!("{url}: http {}", resp.status()));
    }
    resp.into_json()
        .map_err(|e| format!("{url}: decode json: {e}"))
}

fn parse_stats(stats: &serde_json::Value) -> Result<GenStats, String> {
    let get_num = |key: &str| {
        stats
            .get(key)
            .and_then(|v| v.as_f64())
            .ok_or_else(|| format!("stats missing `{key}`"))
    };
    Ok(GenStats {
        prompt_tokens: get_num("prompt_tokens")? as usize,
        prompt_secs: get_num("prompt_secs")?,
        decode_tokens: get_num("decode_tokens")? as usize,
        decode_secs: get_num("decode_secs")?,
    })
}

impl ModelCatalog for RemoteModel {
    fn list(&self) -> Result<Vec<ModelEntry>, String> {
        match self.mode() {
            Mode::OpenAi => {
                let value = Self::get_json(self.url(OPENAI_MODELS_PATH))?;
                let data = value
                    .get("data")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| format!("{OPENAI_MODELS_PATH}: expected `data` array"))?;
                let selected = self.selected();
                Ok(data
                    .iter()
                    .map(|m| ModelEntry {
                        name: m
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        loadable: true,
                        bytes: 0,
                        engine: OPENAI_ENGINE.to_owned(),
                        selected: selected.as_deref() == m.get("id").and_then(|v| v.as_str()),
                    })
                    .collect())
            }
            _ => {
                let value = Self::get_json(self.url(MODELS_PATH))?;
                let entries = value
                    .as_array()
                    .ok_or_else(|| format!("{MODELS_PATH}: expected array"))?;
                entries
                    .iter()
                    .map(|m| {
                        Ok(ModelEntry {
                            name: m
                                .get("name")
                                .and_then(|v| v.as_str())
                                .ok_or("model missing `name`")?
                                .to_string(),
                            loadable: m
                                .get("loadable")
                                .and_then(|v| v.as_bool())
                                .ok_or("model missing `loadable`")?,
                            bytes: m
                                .get("bytes")
                                .and_then(|v| v.as_u64())
                                .ok_or("model missing `bytes`")?,
                            engine: m
                                .get("engine")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string(),
                            selected: m.get("selected").and_then(|v| v.as_bool()).unwrap_or(false),
                        })
                    })
                    .collect()
            }
        }
    }
}

fn openai_submit(
    base: String,
    model: String,
    prompt: String,
    max_tokens: usize,
) -> Result<GenReply, String> {
    let url = format!("{base}{OPENAI_CHAT_PATH}");
    let body = serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
        "max_tokens": max_tokens,
        "stream": false,
    });
    let value = RemoteModel::post_json(url, body)?;
    let text = value
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned();
    let usage = value.get("usage");
    let num = |key: &str| {
        usage
            .and_then(|u| u.get(key))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
    };
    Ok(GenReply {
        model,
        text,
        stats: GenStats {
            prompt_tokens: num("prompt_tokens") as usize,
            prompt_secs: ZERO_SECS,
            decode_tokens: num("completion_tokens") as usize,
            decode_secs: ZERO_SECS,
        },
    })
}

fn legacy_submit(
    base: String,
    prompt: String,
    max_tokens: usize,
    tok: TokKind,
    think: bool,
) -> Result<GenReply, String> {
    let url = format!("{base}{INFERENCE_PATH}");
    let body = serde_json::json!({
        "prompt": prompt,
        "max_tokens": max_tokens,
        "tok": tok.as_str(),
        "think": think,
    });
    let v = RemoteModel::post_json(url, body)?;
    Ok(GenReply {
        model: v
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or_default()
            .to_string(),
        text: v
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string(),
        stats: parse_stats(v.get("stats").unwrap_or(&serde_json::Value::Null))?,
    })
}

impl Inference for RemoteModel {
    fn submit(
        &self,
        prompt: String,
        max_tokens: usize,
        tok: TokKind,
        think: bool,
    ) -> Result<ReplyRx, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let base = self.base_url();
        let mode = self.mode();
        let model = self
            .selected()
            .unwrap_or_else(|| OPENAI_DEFAULT_MODEL.to_owned());
        tokio::task::spawn_blocking(move || {
            let result = match mode {
                Mode::OpenAi => openai_submit(base, model, prompt, max_tokens),
                _ => legacy_submit(base, prompt, max_tokens, tok, think),
            };
            let _ = tx.send(result);
        });
        Ok(rx)
    }
}

impl ModelSwitch for RemoteModel {
    fn select(&self, name: &str) -> Result<(), String> {
        match self.mode() {
            Mode::OpenAi => {
                self.set_selected(Some(name.to_owned()));
                Ok(())
            }
            _ => {
                Self::post_json(self.url(SELECT_PATH), serde_json::json!({ "name": name }))?;
                Ok(())
            }
        }
    }

    fn selected(&self) -> Option<String> {
        match self.mode() {
            Mode::OpenAi => self.selected(),
            _ => {
                let models = ModelCatalog::list(self).ok()?;
                models.into_iter().find(|m| m.selected).map(|m| m.name)
            }
        }
    }
}
