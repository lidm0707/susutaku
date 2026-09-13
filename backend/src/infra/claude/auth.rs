//! Claude OAuth (PKCE) login orchestration: manual code-paste handoff.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

const HOME_ENV: &str = "HOME";
const CLAUDE_HOME_ENV: &str = "CLAUDE_CONFIG_DIR";
const DEFAULT_HOME_SUFFIX: &str = ".claude";

#[derive(Debug, Clone)]
pub enum LoginPhase {
    Idle,
    Awaiting,
    Failed(String),
}

pub struct ClaudeAuth {
    phase: RwLock<LoginPhase>,
    verifier: RwLock<String>,
    /// Agent sandbox workspace used as cwd for `claude -p`.
    workspace: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginStatus {
    LoggedIn,
    Expired,
    Missing,
    AwaitingLogin,
    Failed(String),
}

impl ClaudeAuth {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            phase: RwLock::new(LoginPhase::Idle),
            verifier: RwLock::new(String::new()),
            workspace,
        }
    }

    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// Begin a login: create the PKCE verifier and return the authorize URL.
    /// The user pastes the resulting `code#state` back via `callback`.
    pub fn start(&self) -> Result<String, String> {
        let mut phase = self.phase.write().map_err(|_| "login state poisoned")?;
        if matches!(&*phase, LoginPhase::Awaiting) {
            return Err("login already in progress".into());
        }
        let verifier = claude_cli::new_verifier();
        let url = claude_cli::authorize_url(&verifier);
        *self.verifier.write().unwrap_or_else(|e| e.into_inner()) = verifier.clone();
        *phase = LoginPhase::Awaiting;
        Ok(url)
    }

    /// Exchange the pasted `code` (verifier held from `start`) and persist
    /// credentials under `claude_home`.
    pub async fn callback(self: &Arc<Self>, code: &str) -> Result<(), String> {
        let verifier = self
            .verifier
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if verifier.is_empty() {
            return Err("no login in progress — call start first".into());
        }
        let home = claude_home();
        let code = code.to_string();
        let res = tokio::task::spawn_blocking(move || {
            claude_cli::login_callback(&code, &verifier, &home)
        })
        .await;
        match res {
            Ok(Ok(_)) => {
                self.finish(LoginPhase::Idle);
                Ok(())
            }
            Ok(Err(e)) => {
                let msg = e.to_string();
                self.finish(LoginPhase::Failed(msg.clone()));
                Err(msg)
            }
            Err(e) => {
                let msg = e.to_string();
                self.finish(LoginPhase::Failed(msg.clone()));
                Err(msg)
            }
        }
    }

    /// Combined view: pending login phase first, then the stored token state.
    pub fn status(&self) -> LoginStatus {
        match &*self.phase.read().unwrap_or_else(|e| e.into_inner()) {
            LoginPhase::Awaiting => LoginStatus::AwaitingLogin,
            LoginPhase::Failed(e) => LoginStatus::Failed(e.clone()),
            LoginPhase::Idle => match claude_cli::check(&claude_home()) {
                claude_cli::AuthStatus::LoggedIn => LoginStatus::LoggedIn,
                claude_cli::AuthStatus::Expired => LoginStatus::Expired,
                claude_cli::AuthStatus::Missing => LoginStatus::Missing,
            },
        }
    }

    fn finish(&self, phase: LoginPhase) {
        *self.phase.write().unwrap_or_else(|e| e.into_inner()) = phase;
    }
}

/// `$CLAUDE_CONFIG_DIR` (default `$HOME/.claude`) — shared with the claude CLI.
pub fn claude_home() -> PathBuf {
    if let Ok(dir) = std::env::var(CLAUDE_HOME_ENV) {
        return PathBuf::from(dir);
    }
    let mut home = std::env::var_os(HOME_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.push(DEFAULT_HOME_SUFFIX);
    home
}
