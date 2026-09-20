use codex_cli::model::list;
use codex_cli::model::parse;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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

fn temp_home(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("codex-models-{tag}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
fn missing_cache_lists_empty_not_fallback() {
    let home = temp_home("missing");
    assert!(list(&home).is_empty());
    fs::remove_dir(&home).ok();
}

#[test]
fn lists_models_from_cache_file() {
    let home = temp_home("cache");
    fs::write(home.join("models_cache.json"), SAMPLE).expect("write cache");
    let slugs: Vec<String> = list(&home).into_iter().map(|m| m.slug).collect();
    assert_eq!(slugs, ["gpt-5.6-luna", "gpt-5.5"]);
    fs::remove_dir(&home).ok();
}
