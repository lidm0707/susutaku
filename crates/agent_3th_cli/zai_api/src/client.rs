use std::env;

use ai_interface_layer::error::AiError;
use ai_interface_layer::provider::{ChatProvider, NamedProvider};
use ai_interface_layer::request::ChatRequest;
use ai_interface_layer::response::ChatResponse;
use serde_json::json;

pub const GENERIC_BASE_URL: &str = "https://api.z.ai/api/paas/v4/chat/completions";
pub const CODING_BASE_URL: &str = "https://api.z.ai/api/coding/paas/v4/chat/completions";
pub const BEARER_SCHEME: &str = "Bearer ";
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

/// Which z.ai endpoint a key works against: coding-plan keys only accept
/// `/api/coding/paas/v4`, regular api keys only `/api/paas/v4`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Endpoint {
    #[default]
    Coding,
    Generic,
}

impl Endpoint {
    pub fn url(self) -> &'static str {
        match self {
            Endpoint::Coding => CODING_BASE_URL,
            Endpoint::Generic => GENERIC_BASE_URL,
        }
    }
}

/// Accepts keys pasted with or without the `Bearer ` scheme prefix.
pub fn strip_bearer_scheme(key: &str) -> &str {
    key.strip_prefix(BEARER_SCHEME).unwrap_or(key).trim()
}

pub struct ZaiClient {
    api_key: String,
    model: String,
    endpoint: Endpoint,
}

impl ZaiClient {
    /// Explicit key + model (e.g. from a settings file).
    pub fn from_key(api_key: &str, model: &str) -> Self {
        Self {
            api_key: strip_bearer_scheme(api_key).to_string(),
            model: model.to_string(),
            endpoint: Endpoint::default(),
        }
    }

    /// Key comes from the environment; never hardcoded.
    pub fn from_env() -> Result<Self, AiError> {
        let api_key = env::var(ENV_API_KEY)
            .map_err(|_| AiError::MissingApiKey(ENV_API_KEY))?
            .trim()
            .to_string();
        if api_key.is_empty() {
            return Err(AiError::MissingApiKey(ENV_API_KEY));
        }
        Ok(Self::from_key(&api_key, DEFAULT_MODEL))
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    pub fn with_endpoint(mut self, endpoint: Endpoint) -> Self {
        self.endpoint = endpoint;
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
        let response = ureq::post(self.endpoint.url())
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
