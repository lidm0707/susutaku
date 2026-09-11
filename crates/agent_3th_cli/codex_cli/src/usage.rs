//! ChatGPT backend usage (rate-limit windows) for the logged-in codex
//! account — the same data the CLI `/status` screen shows. Fetched directly
//! from the backend API with the stored OAuth tokens; a 401 triggers one
//! token refresh and retry.

use std::path::Path;

use serde::Deserialize;

use crate::auth;

pub const USAGE_URL: &str = "https://chatgpt.com/backend-api/codex/usage";
const HEADER_AUTHORIZATION: &str = "Authorization";
const HEADER_ACCOUNT_ID: &str = "chatgpt-account-id";
const BEARER_PREFIX: &str = "Bearer ";
const FIELD_RATE_LIMIT: &str = "rate_limit";
const FIELD_PRIMARY: &str = "primary";
const FIELD_SECONDARY: &str = "secondary";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UsageWindow {
    pub used_percent: f64,
    #[serde(default)]
    pub window_minutes: Option<u64>,
    #[serde(default)]
    pub resets_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Usage {
    /// Short window (5h). Carries the reset time.
    pub primary: Option<UsageWindow>,
    /// Long window (weekly). Carries the token usage %.
    pub secondary: Option<UsageWindow>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UsageStatus {
    Ok(Usage),
    NotLoggedIn,
    Unavailable(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FetchError {
    /// Access token rejected — refresh and retry once.
    Unauthorized,
    Other(String),
}

pub fn status(codex_home: &Path) -> UsageStatus {
    let Some(tokens) = auth::load(codex_home) else {
        return UsageStatus::NotLoggedIn;
    };
    match fetch(&tokens) {
        Ok(usage) => UsageStatus::Ok(usage),
        Err(FetchError::Unauthorized) => match auth::refresh(codex_home) {
            Ok(fresh) => to_status(fetch(&fresh)),
            Err(_) => UsageStatus::NotLoggedIn,
        },
        Err(e) => to_status(Err(e)),
    }
}

fn to_status(result: Result<Usage, FetchError>) -> UsageStatus {
    match result {
        Ok(usage) => UsageStatus::Ok(usage),
        Err(FetchError::Unauthorized) => UsageStatus::NotLoggedIn,
        Err(FetchError::Other(e)) => UsageStatus::Unavailable(e),
    }
}

pub fn fetch(tokens: &auth::Tokens) -> Result<Usage, FetchError> {
    let resp = ureq::get(USAGE_URL)
        .set(HEADER_AUTHORIZATION, &format!("{BEARER_PREFIX}{}", tokens.access_token))
        .set(HEADER_ACCOUNT_ID, &tokens.account_id)
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(401 | 403, _) => FetchError::Unauthorized,
            other => FetchError::Other(other.to_string()),
        })?;
    let body = resp
        .into_string()
        .map_err(|e| FetchError::Other(e.to_string()))?;
    parse(&body).map_err(FetchError::Other)
}

/// Lenient parse: `rate_limit` object wrapping both windows, or the windows at
/// the top level. Windows with no `used_percent` are dropped.
pub fn parse(raw: &str) -> Result<Usage, String> {
    let doc: serde_json::Value = serde_json::from_str(raw).map_err(|e| format!("json: {e}"))?;
    let limits = doc.get(FIELD_RATE_LIMIT).unwrap_or(&doc);
    Ok(Usage {
        primary: window(limits, FIELD_PRIMARY),
        secondary: window(limits, FIELD_SECONDARY),
    })
}

fn window(limits: &serde_json::Value, key: &str) -> Option<UsageWindow> {
    let raw = limits.get(key)?;
    let percent = raw
        .get("used_percent")
        .and_then(serde_json::Value::as_f64)?;
    let minutes = raw
        .get("window_minutes")
        .and_then(serde_json::Value::as_u64);
    let resets_at = raw
        .get("resets_at")
        .and_then(serde_json::Value::as_i64)
        .or_else(|| {
            raw.get("reset_after_seconds")
                .and_then(serde_json::Value::as_f64)
                .map(|secs| now_sec() + secs as i64)
        });
    Some(UsageWindow {
        used_percent: percent,
        window_minutes: minutes,
        resets_at,
    })
}

fn now_sec() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
