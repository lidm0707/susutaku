#![allow(clippy::unwrap_used)] // tests: unwrap is the assertion tool

use backend::infra::codex::chat::build_prompt;

#[test]
fn prompt_with_all_parts_is_context_history_message() {
    let p = build_prompt(
        "user: hi\nassistant: hello\n",
        "TASK CONTEXT\ncard #1: t\n",
        "do it",
    );
    assert!(p.starts_with("TASK CONTEXT\ncard #1: t\n\n"));
    assert!(p.contains("conversation so far:\nuser: hi\nassistant: hello\n\n"));
    assert!(p.ends_with("user message:\ndo it"));
}

#[test]
fn prompt_without_history_has_no_header() {
    let p = build_prompt("", "", "hello");
    assert_eq!(p, "user message:\nhello");
    assert!(!p.contains("conversation so far:"));
}

#[test]
fn prompt_history_only() {
    let p = build_prompt("user: hi\n", "", "again");
    assert!(p.starts_with("conversation so far:\nuser: hi\n\n"));
    assert!(p.ends_with("user message:\nagain"));
}
