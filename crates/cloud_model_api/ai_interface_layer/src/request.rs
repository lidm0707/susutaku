use crate::message::Message;

/// Hard cap on messages sent in one request (prompt size guard).
pub const MAX_MESSAGES: usize = 64;
/// Neutral sampling temperature used when a request doesn't specify one.
pub const DEFAULT_TEMPERATURE: f64 = 0.7;

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: f64,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            temperature: DEFAULT_TEMPERATURE,
        }
    }

    /// Single-turn convenience: one user prompt against a model.
    pub fn prompt(model: impl Into<String>, prompt: &str) -> Self {
        Self::new(model, vec![Message::user(prompt)])
    }

    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature;
        self
    }
}
