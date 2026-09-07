//! Codex model catalog, read from the CLI's cached account model list
//! (`$CODEX_HOME/models_cache.json`); hidden models are excluded.

use std::path::Path;

use serde::Deserialize;

pub const MODELS_CACHE_FILE: &str = "models_cache.json";
const VISIBILITY_LIST: &str = "list";
/// Shown when the cache is missing (not logged in yet / older CLI).
const FALLBACK_SLUGS: &[&str] = &["gpt-5-codex", "gpt-5", "gpt-5-mini"];

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
pub fn list(codex_home: &Path) -> Vec<ModelInfo> {
    let raw = std::fs::read_to_string(codex_home.join(MODELS_CACHE_FILE)).unwrap_or_default();
    let parsed = parse(&raw);
    if parsed.is_empty() {
        fallback()
    } else {
        parsed
    }
}

fn parse(raw: &str) -> Vec<ModelInfo> {
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

fn fallback() -> Vec<ModelInfo> {
    FALLBACK_SLUGS
        .iter()
        .map(|slug| ModelInfo {
            slug: (*slug).to_string(),
            display_name: (*slug).to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "models": [
            {"slug":"gpt-reserve","display_name":"GPT-Reserve","visibility":"hide","supported_in_api":true,"priority":3},
            {"slug":"gpt-5.6-luna","display_name":"GPT-5.6-Luna","visibility":"list","supported_in_api":true,"priority":8},
            {"slug":"gpt-5.5","display_name":"GPT-5.5","visibility":"list","supported_in_api":true,"priority":12},
            {"slug":"hidden","display_name":"H","visibility":"list","supported_in_api":false,"priority":1}
        ]
    }"#;

    #[test]
    fn lists_visible_models_sorted_by_priority() {
        let models = parse(SAMPLE);
        let slugs: Vec<&str> = models.iter().map(|m| m.slug.as_str()).collect();
        assert_eq!(slugs, ["gpt-5.6-luna", "gpt-5.5"]);
    }

    #[test]
    fn garbage_cache_yields_empty() {
        assert!(parse("not json").is_empty());
    }
}
