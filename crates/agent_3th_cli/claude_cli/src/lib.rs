//! OAuth (PKCE) login bridge + exec wrapper for the Claude Code CLI on a
//! headless sandbox backend. The browser (frontend) performs the Claude login;
//! the backend exchanges the pasted code, stores
//! `$CLAUDE_CONFIG_DIR/.credentials.json`, and `claude -p` reuses it.

pub mod auth;
pub mod exec;
pub mod pkce;

pub use auth::{authorize_url, check, credentials_path, exchange_code, load, refresh, save};
pub use auth::{AuthError, AuthStatus, Tokens};
pub use exec::{check_available, drain_events, exec_json, ExecError};
pub use pkce::{challenge, new_verifier};

use std::path::Path;

/// Full headless login: exchange the pasted `code` (strip any `#state`) and
/// persist credentials under `claude_home`.
pub fn login_callback(code: &str, verifier: &str, claude_home: &Path) -> Result<Tokens, AuthError> {
    let code = code.split('#').next().unwrap_or(code);
    let tokens = exchange_code(code, verifier)?;
    save(claude_home, &tokens)?;
    Ok(tokens)
}
