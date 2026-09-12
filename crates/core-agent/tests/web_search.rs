use core_agent::toolcall::web_search::{
    MAX_RESULTS, NO_RESULTS_NOTE, SNIPPET_MAX, build_url, parse, request,
};

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
    assert_eq!(results.len(), 5, "got {}: {results:?}", results.len());
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
fn encodes_queries_safely() {
    assert_eq!(
        build_url("rust sqlx enum"),
        "https://api.duckduckgo.com/?q=rust+sqlx+enum&format=json&no_html=1"
    );
    // Reserved characters are percent-encoded.
    let url = build_url("a&b=c?d/e");
    assert!(url.contains("q=a%26b%3Dc%3Fd%2Fe"), "{url}");
    // Non-ASCII is percent-encoded per UTF-8 byte.
    let url = build_url("é");
    assert!(url.contains("q=%C3%A9"), "{url}");
}

#[test]
fn caps_results_at_max() {
    let topics: Vec<String> = (0..10)
        .map(|i| format!(r#"{{"FirstURL":"https://x{i}.example","Text":"Topic {i} - desc"}}"#))
        .collect();
    let body = format!(r#"{{"RelatedTopics":[{}]}}"#, topics.join(","));
    let results = parse(&body);
    assert_eq!(results.len(), MAX_RESULTS);
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

#[test]
fn empty_response_is_empty() {
    assert!(parse("{}").is_empty());
    assert!(parse(r#"{"Heading":"x"}"#).is_empty());
    assert!(parse(r#"{"RelatedTopics":[{"Text":"no url"}]}"#).is_empty());
}

#[test]
fn malformed_response_is_empty_not_panic() {
    assert!(parse("").is_empty());
    assert!(parse("{broken").is_empty());
    assert!(parse("[1,2,3]").is_empty());
}

#[test]
fn request_fails_on_unreachable_url() {
    // Port 1 is reserved; connection must fail and surface as Err, not panic.
    let err = request("http://127.0.0.1:1/").unwrap_err();
    assert!(!err.is_empty());
    assert!(
        !err.to_lowercase().contains("api key"),
        "network failure must not mention API keys: {err}"
    );
}

#[test]
fn no_results_note_mentions_query_not_key() {
    assert!(NO_RESULTS_NOTE.starts_with("No useful DuckDuckGo"));
    assert!(!NO_RESULTS_NOTE.to_lowercase().contains("key"));
}
