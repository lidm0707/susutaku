//! OAuth (PKCE) login bridge + exec wrapper for the codex CLI on a headless
//! sandbox backend. The browser (frontend) performs the ChatGPT login; the
//! backend exchanges the code, stores `$CODEX_HOME/auth.json`, and the CLI
//! reuses/refreshes it on every `codex exec`.

mod auth;
mod exec;
mod model;
mod pkce;

pub use auth::{AuthError, AuthStatus, Tokens};
pub use auth::{account_id_from_id_token, auth_path, authorize_url, check, exchange_code, save};
pub use exec::ExecError;
pub use exec::{check_available, drain_events, exec_json, resolve_bin};
pub use model::ModelInfo;
pub use model::list as list_models;
pub use pkce::{challenge, new_verifier};

use std::path::Path;

/// Full headless login: exchange `code` + `verifier` and persist tokens under `codex_home`.
pub fn login_callback(code: &str, verifier: &str, codex_home: &Path) -> Result<Tokens, AuthError> {
    let tokens = exchange_code(code, verifier)?;
    save(codex_home, &tokens)?;
    Ok(tokens)
}
