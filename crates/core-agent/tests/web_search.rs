use core_agent::toolcall::web_search::{SNIPPET_MAX, parse};

const SAMPLE: &str = r#"{
    "Heading": "MLX",
    "AbstractText": "MLX is an <b>array framework</b> for Apple silicon.",
    "AbstractURL": "https://en.wikipedia.org/wiki/MLX",
    "Answer": "42",
    "AnswerType": "calc",
    "Definition": "",
    "DefinitionURL": "",
    "Results": [{"FirstURL": "https://github.com/ml-explore/mlx", "Text": "MLX on GitHub - official repo"}],
    "RelatedTopics": [
        {"FirstURL": "https://a.example", "Text": "Topic A - about a"},
        {"Topics": [{"FirstURL": "https://b.example", "Text": "Topic B"}]}
    ]
}"#;

#[test]
fn parses_all_sections() {
    let results = parse(SAMPLE);
    assert!(results.len() >= 5, "got {}: {results:?}", results.len());
    assert!(results.iter().any(|r| r.title.starts_with("Answer:")));
    assert!(
        results
            .iter()
            .any(|r| r.url == "https://github.com/ml-explore/mlx")
    );
    assert!(
        results.iter().any(|r| r.url == "https://b.example"),
        "nested topics"
    );
    let abs = results.iter().find(|r| r.title == "MLX").expect("abstract");
    assert_eq!(abs.snippet, "MLX is an array framework for Apple silicon.");
}

#[test]
fn skips_empty_and_truncates() {
    assert!(parse("not json").is_empty());
    let mut long = "x".repeat(SNIPPET_MAX + 50);
    long.insert_str(0, "https://e.example - ");
    let body =
        format!(r#"{{"RelatedTopics":[{{"FirstURL":"https://e.example","Text":"{long}"}}]}}"#);
    let results = parse(&body);
    assert_eq!(results.len(), 1);
    // truncate happens on the text after the title prefix was split off
    assert!(
        results[0].snippet.chars().count() <= SNIPPET_MAX + 1,
        "len {}",
        results[0].snippet.chars().count()
    );
    assert!(results[0].snippet.ends_with('\u{2026}'));
}
