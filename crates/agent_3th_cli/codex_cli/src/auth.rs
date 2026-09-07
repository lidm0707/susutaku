use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::pkce;

const ISSUER: &str = "https://auth.openai.com";
pub const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXu6UxXpqog6";
const REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const SCOPES: &str = "openid profile email offline_access";
const ACCOUNT_ID_CLAIM: &str = "https://api.openai.com/auth.chatgpt_account_id";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStatus {
    LoggedIn,
    Expired,
    Missing,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Tokens {
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
    pub account_id: String,
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

pub fn auth_path(codex_home: &Path) -> PathBuf {
    codex_home.join("auth.json")
}

pub fn authorize_url(verifier: &str) -> String {
    format!(
        "{ISSUER}/oauth/authorize\
?response_type=code&client_id={CLIENT_ID}\
&redirect_uri={REDIRECT_URI}&scope={SCOPES}\
&code_challenge={}&code_challenge_method=S256&prompt=login",
        pkce::challenge(verifier)
    )
}

pub fn exchange_code(code: &str, verifier: &str) -> Result<Tokens, AuthError> {
    let body = serde_json::json!({
        "grant_type": "authorization_code",
        "code": code,
        "redirect_uri": REDIRECT_URI,
        "client_id": CLIENT_ID,
        "code_verifier": verifier,
    });
    let resp: Value = ureq::post(format!("{ISSUER}/oauth/token").as_str())
        .send_json(body)?
        .into_json()?;
    to_tokens(&resp)
}

fn to_tokens(resp: &Value) -> Result<Tokens, AuthError> {
    let get = |k: &'static str| {
        resp.get(k)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or(AuthError::MissingField(k))
    };
    let id_token = get("id_token")?;
    Ok(Tokens {
        account_id: account_id_from_id_token(&id_token)
            .ok_or(AuthError::MissingField("account_id"))?,
        id_token,
        access_token: get("access_token")?,
        refresh_token: get("refresh_token")?,
    })
}

pub fn account_id_from_id_token(id_token: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims
        .get(ACCOUNT_ID_CLAIM)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

pub fn save(codex_home: &Path, tokens: &Tokens) -> Result<(), AuthError> {
    fs::create_dir_all(codex_home)?;
    let doc = serde_json::json!({
        "OPENAI_API_KEY": Value::Null,
        "tokens": tokens,
        "last_refresh": chrono_now(),
    });
    let path = auth_path(codex_home);
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

fn chrono_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

pub fn check(codex_home: &Path) -> AuthStatus {
    let Ok(bytes) = fs::read(auth_path(codex_home)) else {
        return AuthStatus::Missing;
    };
    let Ok(doc) = serde_json::from_slice::<Value>(&bytes) else {
        return AuthStatus::Missing;
    };
    let Some(access) = doc.pointer("/tokens/access_token").and_then(Value::as_str) else {
        return AuthStatus::Missing;
    };
    match access_expired(access) {
        Some(true) => AuthStatus::Expired,
        _ => AuthStatus::LoggedIn,
    }
}

/// `Some(expired)` from the JWT `exp` claim; `None` if undecodable (treat as valid).
fn access_expired(access_token: &str) -> Option<bool> {
    let payload = access_token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    let exp = claims.get("exp")?.as_i64()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    Some(exp <= now)
}
