//! Codex model catalog, read from the CLI's cached account model list
//! (`$CODEX_HOME/models_cache.json`); hidden models are excluded.

use std::path::Path;

use serde::Deserialize;

pub const MODELS_CACHE_FILE: &str = "models_cache.json";
const VISIBILITY_LIST: &str = "list";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub slug: String,
    pub display_name: String,
}

#[derive(Deserialize)]
struct Cache {
    models: Vec<CacheModel>,
}

#[derive(Deserialize)]
struct CacheModel {
    slug: String,
    display_name: String,
    #[serde(default)]
    visibility: String,
    #[serde(default)]
    supported_in_api: bool,
    #[serde(default)]
    priority: i64,
}

/// Visible models for the logged-in account, cheapest-priority first.
/// Empty when the CLI cache is missing (not logged in yet / older CLI).
pub fn list(codex_home: &Path) -> Vec<ModelInfo> {
    let raw = std::fs::read_to_string(codex_home.join(MODELS_CACHE_FILE)).unwrap_or_default();
    parse(&raw)
}

pub fn parse(raw: &str) -> Vec<ModelInfo> {
    let Ok(cache) = serde_json::from_str::<Cache>(raw) else {
        return Vec::new();
    };
    let mut models: Vec<(i64, ModelInfo)> = cache
        .models
        .iter()
        .filter(|m| m.visibility == VISIBILITY_LIST && m.supported_in_api)
        .map(|m| {
            (
                m.priority,
                ModelInfo {
                    slug: m.slug.clone(),
                    display_name: m.display_name.clone(),
                },
            )
        })
        .collect();
    models.sort_by_key(|(priority, _)| *priority);
    models.into_iter().map(|(_, info)| info).collect()
}
