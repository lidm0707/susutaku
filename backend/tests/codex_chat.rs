use backend::infra::codex_chat::extract;

#[test]
fn extracts_agent_message_new_and_old_shapes() {
    let mut text = String::new();
    let mut err = String::new();
    extract(
        r#"{"id":"1","item":{"type":"agent_message","text":"hello"}}"#,
        &mut text,
        &mut err,
    );
    extract(
        r#"{"msg":{"type":"agent_message","message":"world"}}"#,
        &mut text,
        &mut err,
    );
    extract(
        r#"{"msg":{"type":"error","message":"boom"}}"#,
        &mut text,
        &mut err,
    );
    assert_eq!(text, "helloworld");
    assert_eq!(err, "boom");
}

#[test]
fn ignores_non_json_lines() {
    let mut text = String::new();
    let mut err = String::new();
    extract("not json", &mut text, &mut err);
    assert!(text.is_empty() && err.is_empty());
}
