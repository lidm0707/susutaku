use crate::error::AiError;
use crate::request::ChatRequest;
use crate::response::ChatResponse;

/// Marker for providers that carry their own model identifier.
pub trait NamedProvider {
    fn name(&self) -> &'static str;
    fn default_model(&self) -> &'static str;
}

/// The single seam every third-party AI backend implements.
pub trait ChatProvider {
    fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, AiError>;
}

/// One-shot helper used by CLI front-ends: prompt in, completion text out.
pub fn run_prompt(
    provider: &(impl ChatProvider + NamedProvider),
    prompt: &str,
) -> Result<String, AiError> {
    let request = ChatRequest::prompt(provider.default_model(), prompt);
    provider.complete(&request).map(|response| response.content)
}
