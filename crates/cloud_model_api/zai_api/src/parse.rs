use ai_interface_layer::error::AiError;
use ai_interface_layer::message::Role;
use ai_interface_layer::request::ChatRequest;
use serde_json::{Value, json};

pub const ROLE_FIELD: &str = "role";
pub const CONTENT_FIELD: &str = "content";
pub const CHOICES_FIELD: &str = "choices";
pub const MESSAGE_FIELD: &str = "message";
pub const TYPE_FIELD: &str = "type";
pub const DELTA_FIELD: &str = "delta";
pub const TEXT_PART: &str = "text";
pub const IMAGE_PART: &str = "image_url";
pub const IMAGE_URL_FIELD: &str = "url";

/// Serialize the conversation for the OpenAI-compatible chat body. Messages
/// carrying an image become multimodal content-part arrays.
pub fn messages_value(request: &ChatRequest) -> Value {
    Value::Array(
        request
            .messages
            .iter()
            .map(|m| match &m.image {
                None => json!({
                    ROLE_FIELD: m.role.as_str(),
                    CONTENT_FIELD: m.content,
                }),
                Some(url) => json!({
                    ROLE_FIELD: m.role.as_str(),
                    CONTENT_FIELD: [
                        { TYPE_FIELD: TEXT_PART, TEXT_PART: m.content },
                        { TYPE_FIELD: IMAGE_PART, IMAGE_PART: { IMAGE_URL_FIELD: url } },
                    ],
                }),
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

/// Pull the first choice's streamed delta content out of an SSE chunk.
/// Chunks without content (role-only, tool calls, keep-alives) yield `None`.
pub fn extract_delta(raw: &str) -> Option<String> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let delta = value[CHOICES_FIELD]
        .as_array()
        .and_then(|choices| choices.first())?[DELTA_FIELD][CONTENT_FIELD]
        .as_str()?
        .to_string();
    if delta.is_empty() { None } else { Some(delta) }
}

pub fn role_from_str(raw: &str) -> Role {
    match raw {
        "system" => Role::System,
        "assistant" => Role::Assistant,
        _ => Role::User,
    }
}
