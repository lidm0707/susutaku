use std::env;

use ai_interface_layer::{AiError, ChatProvider, ChatRequest, ChatResponse, NamedProvider};
use serde_json::json;

pub const BASE_URL: &str = "https://api.z.ai/api/paas/v4/chat/completions";
pub const ENV_API_KEY: &str = "ZAI_API_KEY";
pub const PROVIDER_NAME: &str = "zai";
pub const DEFAULT_MODEL: &str = "glm-4.6";
pub const AUTH_HEADER: &str = "Authorization";
pub const BEARER_PREFIX: &str = "Bearer ";
pub const CONTENT_TYPE: &str = "Content-Type";
pub const JSON_CONTENT_TYPE: &str = "application/json";
pub const CHOICES_FIELD: &str = "choices";
pub const MESSAGE_FIELD: &str = "message";
pub const CONTENT_FIELD: &str = "content";
pub const EMPTY_BODY_NOTE: &str = "<unreadable body>";

pub struct ZaiClient {
    api_key: String,
    model: String,
}

impl ZaiClient {
    /// Key comes from the environment; never hardcoded.
    pub fn from_env() -> Result<Self, AiError> {
        let api_key = env::var(ENV_API_KEY)
            .map_err(|_| AiError::MissingApiKey(ENV_API_KEY))?
            .trim()
            .to_string();
        if api_key.is_empty() {
            return Err(AiError::MissingApiKey(ENV_API_KEY));
        }
        Ok(Self {
            api_key,
            model: DEFAULT_MODEL.to_string(),
        })
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    /// Fill in the concrete model when the request leaves it unset.
    fn effective_model<'a>(&'a self, request: &'a ChatRequest) -> &'a str {
        if request.model.is_empty() {
            &self.model
        } else {
            &request.model
        }
    }

    fn send(&self, body: &str) -> Result<String, AiError> {
        let response = ureq::post(BASE_URL)
            .set(CONTENT_TYPE, JSON_CONTENT_TYPE)
            .set(AUTH_HEADER, &format!("{BEARER_PREFIX}{}", self.api_key))
            .send_string(body)
            .map_err(|e| match e {
                ureq::Error::Status(code, response) => AiError::Status(
                    code,
                    response
                        .into_string()
                        .unwrap_or_else(|_| EMPTY_BODY_NOTE.to_string()),
                ),
                other => AiError::Http(other.to_string()),
            })?;
        response
            .into_string()
            .map_err(|e| AiError::Http(e.to_string()))
    }
}

impl NamedProvider for ZaiClient {
    fn name(&self) -> &'static str {
        PROVIDER_NAME
    }

    fn default_model(&self) -> &'static str {
        DEFAULT_MODEL
    }
}

impl ChatProvider for ZaiClient {
    fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, AiError> {
        let body = json!({
            "model": self.effective_model(request),
            "messages": crate::parse::messages_value(request),
            "temperature": request.temperature,
        })
        .to_string();
        let raw = self.send(&body)?;
        let content = crate::parse::extract_content(&raw)?;
        Ok(ChatResponse { content })
    }
}
