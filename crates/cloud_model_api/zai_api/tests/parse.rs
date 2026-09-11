use ai_interface_layer::error::AiError;
use ai_interface_layer::message::{Message, Role};
use ai_interface_layer::request::ChatRequest;
use zai_api::parse::{extract_content, messages_value, role_from_str, CONTENT_FIELD, ROLE_FIELD};

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
