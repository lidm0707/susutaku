use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::pkce;

pub const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const AUTHORIZE_URL: &str = "https://claude.ai/oauth/authorize";
const TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";
/// Console-hosted callback: shows the `code#state` in the page for manual
/// handoff — no localhost listener needed, ideal for a headless sandbox.
const REDIRECT_URI: &str = "https://console.anthropic.com/oauth/code/callback";
const SCOPES: &str = "org:create_api_key user:profile user:inference";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStatus {
    LoggedIn,
    Expired,
    Missing,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at_millis: u64,
    pub scopes: Vec<String>,
}

#[derive(Debug)]
pub enum AuthError {
    Http(Box<ureq::Error>),
    Io(std::io::Error),
    Json(serde_json::Error),
    MissingField(&'static str),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Http(e) => write!(f, "http: {e}"),
            AuthError::Io(e) => write!(f, "io: {e}"),
            AuthError::Json(e) => write!(f, "json: {e}"),
            AuthError::MissingField(k) => write!(f, "token response missing field `{k}`"),
        }
    }
}

impl std::error::Error for AuthError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AuthError::Http(e) => Some(e.as_ref()),
            AuthError::Io(e) => Some(e),
            AuthError::Json(e) => Some(e),
            AuthError::MissingField(_) => None,
        }
    }
}

impl From<ureq::Error> for AuthError {
    fn from(e: ureq::Error) -> Self {
        AuthError::Http(Box::new(e))
    }
}

impl From<std::io::Error> for AuthError {
    fn from(e: std::io::Error) -> Self {
        AuthError::Io(e)
    }
}

impl From<serde_json::Error> for AuthError {
    fn from(e: serde_json::Error) -> Self {
        AuthError::Json(e)
    }
}

pub fn credentials_path(claude_home: &Path) -> PathBuf {
    claude_home.join(".credentials.json")
}

pub fn authorize_url(verifier: &str) -> String {
    format!(
        "{AUTHORIZE_URL}?response_type=code&client_id={CLIENT_ID}\
&redirect_uri={REDIRECT_URI}&scope={SCOPES}\
&code_challenge={}&code_challenge_method=S256",
        pkce::challenge(verifier)
    )
}

/// The user pastes `code` (strip any `#state` suffix before calling).
pub fn exchange_code(code: &str, verifier: &str) -> Result<Tokens, AuthError> {
    let body = serde_json::json!({
        "grant_type": "authorization_code",
        "code": code,
        "redirect_uri": REDIRECT_URI,
        "client_id": CLIENT_ID,
        "code_verifier": verifier,
    });
    let resp: Value = ureq::post(TOKEN_URL).send_json(body)?.into_json()?;
    to_tokens(resp)
}

/// Refresh flow so long-lived sandboxes stay authenticated without re-login.
pub fn refresh(refresh_token: &str) -> Result<Tokens, AuthError> {
    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "client_id": CLIENT_ID,
    });
    let resp: Value = ureq::post(TOKEN_URL).send_json(body)?.into_json()?;
    to_tokens(resp)
}

fn to_tokens(resp: Value) -> Result<Tokens, AuthError> {
    let get = |k: &'static str| {
        resp.get(k)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or(AuthError::MissingField(k))
    };
    let expires_in: u64 = resp
        .get("expires_in")
        .and_then(Value::as_u64)
        .ok_or(AuthError::MissingField("expires_in"))?;
    let scopes = resp
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or(SCOPES)
        .split(' ')
        .map(str::to_owned)
        .collect();
    Ok(Tokens {
        access_token: get("access_token")?,
        refresh_token: get("refresh_token")?,
        expires_at_millis: unix_millis() + expires_in * 1000,
        scopes,
    })
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

pub fn save(claude_home: &Path, tokens: &Tokens) -> Result<(), AuthError> {
    fs::create_dir_all(claude_home)?;
    let doc = serde_json::json!({ "claudeAiOauth": tokens });
    let path = credentials_path(claude_home);
    fs::write(&path, serde_json::to_vec(&doc)?)?;
    set_owner_only(&path);
    Ok(())
}

fn set_owner_only(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        const OWNER_ONLY: u32 = 0o600;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(OWNER_ONLY));
    }
}

pub fn load(claude_home: &Path) -> Result<Tokens, AuthError> {
    let bytes = fs::read(credentials_path(claude_home))?;
    let doc: Value = serde_json::from_slice(&bytes)?;
    serde_json::from_value(
        doc.get("claudeAiOauth")
            .cloned()
            .ok_or(AuthError::MissingField("claudeAiOauth"))?,
    )
    .map_err(AuthError::from)
}

pub fn check(claude_home: &Path) -> AuthStatus {
    let Ok(tokens) = load(claude_home) else {
        return AuthStatus::Missing;
    };
    if tokens.expires_at_millis <= unix_millis() {
        AuthStatus::Expired
    } else {
        AuthStatus::LoggedIn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorize_url_contains_pkce() {
        let url = authorize_url("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert!(url.contains("code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"));
        assert!(url.contains(CLIENT_ID));
        assert!(url.contains("claude.ai/oauth/authorize"));
    }

    #[test]
    fn to_tokens_roundtrip() {
        let resp = serde_json::json!({
            "access_token": "at", "refresh_token": "rt",
            "expires_in": 3600, "scope": "user:profile user:inference"
        });
        let tokens = to_tokens(resp).unwrap();
        assert_eq!(tokens.access_token, "at");
        assert_eq!(tokens.scopes, vec!["user:profile", "user:inference"]);
        assert!(tokens.expires_at_millis > unix_millis());
    }
}
