//! Remote provider client: model-server HTTP adapter.

use susutaku_mlx::engine::GenStats;
use susutaku_mlx::tok::TokKind;

use crate::port::outbound::{GenReply, Inference, ModelSwitch, ReplyRx};

const HTTP_OK_RANGE: std::ops::Range<u16> = 200..300;
const MODELS_PATH: &str = "/api/models";
const SELECT_PATH: &str = "/api/models/select";
const INFERENCE_PATH: &str = "/api/inference";

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

pub struct RemoteModel {
    base_url: String,
}

impl RemoteModel {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
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

impl Inference for RemoteModel {
    fn submit(
        &self,
        prompt: String,
        max_tokens: usize,
        tok: TokKind,
        think: bool,
    ) -> Result<ReplyRx, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let url = self.url(INFERENCE_PATH);
        let body = serde_json::json!({
            "prompt": prompt,
            "max_tokens": max_tokens,
            "tok": tok.as_str(),
            "think": think,
        });
        tokio::task::spawn_blocking(move || {
            let result = Self::post_json(url, body).and_then(|v| {
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
            });
            let _ = tx.send(result);
        });
        Ok(rx)
    }
}

impl ModelSwitch for RemoteModel {
    fn select(&self, name: &str) -> Result<(), String> {
        Self::post_json(self.url(SELECT_PATH), serde_json::json!({ "name": name }))?;
        Ok(())
    }

    fn selected(&self) -> Option<String> {
        let models = ModelCatalog::list(self).ok()?;
        models.into_iter().find(|m| m.selected).map(|m| m.name)
    }
}
