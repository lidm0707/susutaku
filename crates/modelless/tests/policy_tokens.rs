//! Policy corpus + shared katgpt tokenizer tests.

use modelless::policy::{decode, encode, policy_text, shared_tokenizer, token_count};

#[test]
fn corpus_contains_tool_lines_and_verbs() {
    let text = policy_text();
    assert!(text.contains("TOOL: SHELL cargo check"));
    assert!(text.contains("{\"tool\": \"git\", \"op\": \"STATUS\"}"));
    // verb corpus rides along
    assert!(text.contains("deleted"));
}

#[test]
fn token_counts_are_positive_and_monotone() {
    let short = token_count("TOOL: GIT STATUS");
    let long = token_count(
        "TOOL: SHELL cargo check && cargo test --workspace && git push origin task/1-agent",
    );
    assert!(short > 0);
    assert!(long > short, "longer text must not count fewer tokens");
}

#[test]
fn tokenize_then_decode_roundtrips_the_call() {
    let call = "{\"tool\": \"git\", \"op\": \"COMMIT\", \"message\": \"agent run\"}";
    assert_eq!(decode(&encode(call)), call);
}

#[test]
fn shared_tokenizer_is_the_same_instance() {
    let a = shared_tokenizer() as *const _;
    let b = shared_tokenizer() as *const _;
    assert_eq!(a, b, "OnceLock must hand out the same trained BPE");
}
