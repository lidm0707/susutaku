use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::pkce;
use rand::RngCore;

const ISSUER: &str = "https://auth.openai.com";

/// The OAuth client id is never hardcoded: it is recovered from the official
/// codex CLI's own tokens (`aud` claim).
fn client_id() -> Result<String, AuthError> {
    client_id_from_codex_home(&codex_home()).ok_or(AuthError::MissingClientId)
}

/// `$CODEX_HOME` (default `$HOME/.codex`) — same resolution the CLI uses.
fn codex_home() -> PathBuf {
    const CODEX_HOME_ENV: &str = "CODEX_HOME";
    const HOME_ENV: &str = "HOME";
    const DEFAULT_HOME_SUFFIX: &str = ".codex";
    if let Ok(dir) = std::env::var(CODEX_HOME_ENV) {
        return PathBuf::from(dir);
    }
    let mut home = std::env::var_os(HOME_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.push(DEFAULT_HOME_SUFFIX);
    home
}

pub fn client_id_from_codex_home(codex_home: &Path) -> Option<String> {
    let bytes = fs::read(auth_path(codex_home)).ok()?;
    let doc = serde_json::from_slice::<Value>(&bytes).ok()?;
    let id_token = doc.pointer("/tokens/id_token")?.as_str()?;
    audience_from_id_token(id_token)
}

/// First `aud` entry of the id_token JWT payload.
pub fn audience_from_id_token(id_token: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    match claims.get("aud")? {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) => items.first()?.as_str().map(str::to_owned),
        _ => None,
    }
}
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
    /// Token endpoint rejected the exchange: (status, provider error body).
    Token(u16, String),
    Io(std::io::Error),
    Json(serde_json::Error),
    MissingField(&'static str),
    /// No codex CLI login to recover the client id from.
    MissingClientId,
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Http(e) => write!(f, "http: {e}"),
            AuthError::Token(status, body) => {
                write!(f, "token endpoint returned {status}: {body}")
            }
            AuthError::Io(e) => write!(f, "io: {e}"),
            AuthError::Json(e) => write!(f, "json: {e}"),
            AuthError::MissingField(k) => write!(f, "token response missing field `{k}`"),
            AuthError::MissingClientId => {
                write!(f, "codex client id unknown: log in with the codex CLI once")
            }
        }
    }
}

impl std::error::Error for AuthError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AuthError::Http(e) => Some(e.as_ref()),
            AuthError::Token(..) => None,
            AuthError::Io(e) => Some(e),
            AuthError::Json(e) => Some(e),
            AuthError::MissingField(_) => None,
            AuthError::MissingClientId => None,
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

const STATE_BYTES: usize = 16;

pub fn authorize_url(verifier: &str) -> Result<String, AuthError> {
    Ok(format!(
        "{ISSUER}/oauth/authorize\
?response_type=code&client_id={}\
&redirect_uri={REDIRECT_URI}&scope={SCOPES}\
&code_challenge={}&code_challenge_method=S256&prompt=login\
&state={}",
        client_id()?,
        pkce::challenge(verifier),
        new_state(),
    ))
}

fn new_state() -> String {
    let mut bytes = [0u8; STATE_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn exchange_code(code: &str, verifier: &str) -> Result<Tokens, AuthError> {
    let body = serde_json::json!({
        "grant_type": "authorization_code",
        "code": code,
        "redirect_uri": REDIRECT_URI,
        "client_id": client_id()?,
        "code_verifier": verifier,
    });
    let resp = ureq::post(format!("{ISSUER}/oauth/token").as_str())
        .send_json(body)
        .map_err(|e| match e {
            ureq::Error::Status(status, resp) => {
                let text = resp.into_string().unwrap_or_default();
                AuthError::Token(status, text)
            }
            other => AuthError::from(other),
        })?
        .into_json::<Value>()?;
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

pub fn load(codex_home: &Path) -> Option<Tokens> {
    let bytes = fs::read(auth_path(codex_home)).ok()?;
    let doc: Value = serde_json::from_slice(&bytes).ok()?;
    Some(Tokens {
        id_token: doc.pointer("/tokens/id_token")?.as_str()?.to_owned(),
        access_token: doc.pointer("/tokens/access_token")?.as_str()?.to_owned(),
        refresh_token: doc.pointer("/tokens/refresh_token")?.as_str()?.to_owned(),
        account_id: doc.pointer("/tokens/account_id")?.as_str()?.to_owned(),
    })
}

const GRANT_REFRESH: &str = "refresh_token";

/// Exchange the stored refresh token for fresh tokens and persist them.
pub fn refresh(codex_home: &Path) -> Result<Tokens, AuthError> {
    let tokens = load(codex_home).ok_or(AuthError::MissingField("refresh_token"))?;
    let body = serde_json::json!({
        "grant_type": GRANT_REFRESH,
        "refresh_token": tokens.refresh_token,
        "client_id": client_id()?,
    });
    let resp = ureq::post(format!("{ISSUER}/oauth/token").as_str())
        .send_json(body)
        .map_err(|e| match e {
            ureq::Error::Status(status, resp) => {
                let text = resp.into_string().unwrap_or_default();
                AuthError::Token(status, text)
            }
            other => AuthError::from(other),
        })?
        .into_json::<Value>()?;
    let fresh = to_tokens(&resp)?;
    save(codex_home, &fresh)?;
    Ok(fresh)
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
