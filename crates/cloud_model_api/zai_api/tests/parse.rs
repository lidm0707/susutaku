use ai_interface_layer::error::AiError;
use ai_interface_layer::message::{Message, Role};
use ai_interface_layer::request::ChatRequest;
use zai_api::parse::{
    CONTENT_FIELD, ROLE_FIELD, extract_content, extract_delta, messages_value, role_from_str,
};

const SAMPLE: &str = r#"{
    "choices": [
        {"message": {"role": "assistant", "content": "hello"}}
    ]
}"#;

#[test]
fn extracts_first_choice_content() {
    assert_eq!(extract_content(SAMPLE).unwrap(), "hello");
}

#[test]
fn empty_and_bad_payloads_error() {
    assert!(matches!(
        extract_content("not json"),
        Err(AiError::Parse(_))
    ));
    assert_eq!(
        extract_content(r#"{"choices": []}"#).unwrap_err(),
        AiError::EmptyResponse
    );
}

#[test]
fn serializes_roles() {
    let request = ChatRequest::new(
        "glm-4.6",
        vec![Message::system("be brief"), Message::user("hi")],
    );
    let value = messages_value(&request);
    assert_eq!(value[0][ROLE_FIELD], "system");
    assert_eq!(value[1][CONTENT_FIELD], "hi");
}

#[test]
fn maps_role_strings() {
    assert_eq!(role_from_str("assistant"), Role::Assistant);
    assert_eq!(role_from_str("junk"), Role::User);
}

const DELTA_CHUNK: &str = r#"{
    "choices": [{"delta": {"content": "he"}}]
}"#;

#[test]
fn extracts_stream_delta_content() {
    assert_eq!(extract_delta(DELTA_CHUNK).as_deref(), Some("he"));
}

#[test]
fn skips_non_content_chunks() {
    let role_only = r#"{"choices": [{"delta": {"role": "assistant", "content": ""}}]}"#;
    let finish = r#"{"choices": [{"delta": {}, "finish_reason": "stop"}]}"#;
    let bad_json = "not json";
    assert_eq!(extract_delta(role_only), None);
    assert_eq!(extract_delta(finish), None);
    assert_eq!(extract_delta(bad_json), None);
}
