use susutaku_mlx::json_guard::{GuardState, JsonGuard};

#[test]
fn accepts_complete_json() {
    let mut g = JsonGuard::new();
    g.feed(r#"{"a": 1, "b": [1, 2, 3]}"#);
    assert_eq!(g.state(), GuardState::Balanced);
}

#[test]
fn delimiters_inside_strings_do_not_count() {
    let mut g = JsonGuard::new();
    g.feed(r#"{"text": "} ] { ["#);
    assert_eq!(g.state(), GuardState::Open);
    g.feed(r#""}"#);
    assert_eq!(g.state(), GuardState::Balanced);
}

#[test]
fn escaped_quote_keeps_string_open() {
    let mut g = JsonGuard::new();
    g.feed(r#"{"s": "he said \"hi\""}"#);
    assert_eq!(g.state(), GuardState::Balanced);
}

#[test]
fn open_object_stays_open() {
    let mut g = JsonGuard::new();
    g.feed(r#"{"a": {"b": 1"#);
    assert_eq!(g.state(), GuardState::Open);
    g.feed("}}");
    assert_eq!(g.state(), GuardState::Balanced);
}

#[test]
fn plain_text_never_balances() {
    let mut g = JsonGuard::new();
    g.feed("Sure, here is your answer");
    assert_eq!(g.state(), GuardState::Open);
}

#[test]
fn extra_closing_is_broken() {
    let mut g = JsonGuard::new();
    g.feed(r#"{"a": 1}}"#);
    assert_eq!(g.state(), GuardState::Broken);
}

#[test]
fn unterminated_string_is_open() {
    let mut g = JsonGuard::new();
    g.feed(r#"{"a": "unterminated"#);
    assert_eq!(g.state(), GuardState::Open);
}

#[test]
fn multibyte_string_content_is_safe() {
    let mut g = JsonGuard::new();
    g.feed("{\"s\": \"日本語 } 語\", \"n\": 2}");
    assert_eq!(g.state(), GuardState::Balanced);
}

#[test]
fn incremental_feed_matches_single_feed() {
    let full = r#"{"tool": "web_search", "args": {"q": "a[b]c"}, "n": 3}"#;
    let mut one = JsonGuard::new();
    one.feed(full);
    let mut inc = JsonGuard::new();
    for part in full.as_bytes().chunks(3) {
        inc.feed(std::str::from_utf8(part).unwrap());
    }
    assert_eq!(one.state(), inc.state());
    assert_eq!(inc.state(), GuardState::Balanced);
}
