use codex_cli::model::parse;

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
