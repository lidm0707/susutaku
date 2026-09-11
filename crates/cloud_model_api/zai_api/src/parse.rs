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
