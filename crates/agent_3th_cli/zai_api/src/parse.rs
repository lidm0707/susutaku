use ai_interface_layer::error::AiError;
use ai_interface_layer::message::Role;
use ai_interface_layer::request::ChatRequest;
use serde_json::{Value, json};

pub const ROLE_FIELD: &str = "role";
pub const CONTENT_FIELD: &str = "content";
pub const CHOICES_FIELD: &str = "choices";
pub const MESSAGE_FIELD: &str = "message";

/// Serialize the conversation for the OpenAI-compatible chat body.
pub fn messages_value(request: &ChatRequest) -> Value {
    Value::Array(
        request
            .messages
            .iter()
            .map(|m| {
                json!({
                    ROLE_FIELD: m.role.as_str(),
                    CONTENT_FIELD: m.content,
                })
            })
            .collect(),
    )
}

/// Pull the first choice's message content out of a completion payload.
pub fn extract_content(raw: &str) -> Result<String, AiError> {
    let value: Value = serde_json::from_str(raw).map_err(|e| AiError::Parse(e.to_string()))?;
    value[CHOICES_FIELD]
        .as_array()
        .and_then(|choices| choices.first())
        .and_then(|choice| choice[MESSAGE_FIELD][CONTENT_FIELD].as_str())
        .map(str::to_string)
        .ok_or(AiError::EmptyResponse)
}

pub fn role_from_str(raw: &str) -> Role {
    match raw {
        "system" => Role::System,
        "assistant" => Role::Assistant,
        _ => Role::User,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_interface_layer::message::Message;
    use ai_interface_layer::request::ChatRequest;

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
}
