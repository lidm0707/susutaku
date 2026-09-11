//! OAuth (PKCE) login bridge + exec wrapper for the codex CLI on a headless
//! sandbox backend. The browser (frontend) performs the ChatGPT login; the
//! backend exchanges the code, stores `$CODEX_HOME/auth.json`, and the CLI
//! reuses/refreshes it on every `codex exec`.

pub mod auth;
pub mod exec;
pub mod model;
pub mod pkce;
pub mod usage;

pub use auth::{AuthError, AuthStatus, Tokens};
pub use auth::{authorize_url, check, client_id_from_codex_home, exchange_code, save};
pub use exec::ExecError;
pub use exec::{check_available, drain_events, exec_json, login, resolve_bin};
pub use model::ModelInfo;
pub use model::list as list_models;
pub use pkce::{challenge, new_verifier};
pub use usage::{FetchError, Usage, UsageStatus, UsageWindow};
pub use usage::{fetch as fetch_usage, parse as parse_usage, status as usage_status};

use std::path::Path;

/// Full headless login: exchange `code` + `verifier` and persist tokens under `codex_home`.
pub fn login_callback(code: &str, verifier: &str, codex_home: &Path) -> Result<Tokens, AuthError> {
    let tokens = exchange_code(code, verifier)?;
    save(codex_home, &tokens)?;
    Ok(tokens)
}
