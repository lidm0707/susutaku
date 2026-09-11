use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiError {
    MissingApiKey(&'static str),
    Http(String),
    Status(u16, String),
    Parse(String),
    EmptyResponse,
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AiError::MissingApiKey(env) => write!(f, "missing api key env: {env}"),
            AiError::Http(e) => write!(f, "http error: {e}"),
            AiError::Status(code, body) => write!(f, "http {code}: {body}"),
            AiError::Parse(e) => write!(f, "parse error: {e}"),
            AiError::EmptyResponse => write!(f, "empty response"),
        }
    }
}

impl std::error::Error for AiError {}
