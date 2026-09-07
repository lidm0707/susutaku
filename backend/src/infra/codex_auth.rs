//! Codex OAuth login orchestration: loopback callback listener + state.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const CALLBACK_PORT: u16 = 1455;
const LOGIN_TIMEOUT: Duration = Duration::from_secs(600);
const HTTP_OK: &str = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n";
const CALLBACK_HTML: &str =
    "<html><body><p>Codex login complete. You can close this tab.</p></body></html>";
const MAX_REQUEST_BYTES: usize = 8192;
const CODE_PREFIX: &str = "code=";

#[derive(Debug, Clone)]
pub enum LoginPhase {
    Idle,
    Awaiting,
    Failed(String),
}

pub struct CodexAuth {
    phase: RwLock<LoginPhase>,
    /// Agent sandbox workspace used as cwd for `codex exec`.
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

impl CodexAuth {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            phase: RwLock::new(LoginPhase::Idle),
            workspace,
        }
    }

    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// Begin a login: create the PKCE verifier, start the loopback listener,
    /// return the authorize URL for the browser.
    pub fn start(self: &Arc<Self>) -> Result<String, String> {
        let mut phase = self.phase.write().map_err(|_| "login state poisoned")?;
        if matches!(&*phase, LoginPhase::Awaiting) {
            return Err("login already in progress".into());
        }
        let verifier = codex_cli::new_verifier();
        let url = codex_cli::authorize_url(&verifier);
        *phase = LoginPhase::Awaiting;
        drop(phase);
        tokio::spawn(self.clone().run(verifier));
        Ok(url)
    }

    /// Combined view: pending login phase first, then the stored token state.
    pub fn status(&self) -> LoginStatus {
        match &*self.phase.read().unwrap_or_else(|e| e.into_inner()) {
            LoginPhase::Awaiting => LoginStatus::AwaitingLogin,
            LoginPhase::Failed(e) => LoginStatus::Failed(e.clone()),
            LoginPhase::Idle => match codex_cli::check(&codex_home()) {
                codex_cli::AuthStatus::LoggedIn => LoginStatus::LoggedIn,
                codex_cli::AuthStatus::Expired => LoginStatus::Expired,
                codex_cli::AuthStatus::Missing => LoginStatus::Missing,
            },
        }
    }

    async fn run(self: Arc<Self>, verifier: String) {
        let code = match callback_code().await {
            Ok(code) => code,
            Err(e) => {
                self.finish(LoginPhase::Failed(e));
                return;
            }
        };
        let home = codex_home();
        let res =
            tokio::task::spawn_blocking(move || codex_cli::login_callback(&code, &verifier, &home))
                .await;
        match res {
            Ok(Ok(_)) => self.finish(LoginPhase::Idle),
            Ok(Err(e)) => self.finish(LoginPhase::Failed(e.to_string())),
            Err(e) => self.finish(LoginPhase::Failed(e.to_string())),
        }
    }

    fn finish(&self, phase: LoginPhase) {
        *self.phase.write().unwrap_or_else(|e| e.into_inner()) = phase;
    }
}

/// Accept exactly one browser redirect on the loopback callback port and
/// return the `code` query parameter.
async fn callback_code() -> Result<String, String> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, CALLBACK_PORT));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("cannot bind callback port {CALLBACK_PORT}: {e}"))?;
    let (mut conn, _) = tokio::time::timeout(LOGIN_TIMEOUT, listener.accept())
        .await
        .map_err(|_| "login timed out".to_string())?
        .map_err(|e| format!("callback accept failed: {e}"))?;

    let mut buf = [0u8; MAX_REQUEST_BYTES];
    let n = conn
        .read(&mut buf)
        .await
        .map_err(|e| format!("callback read failed: {e}"))?;
    let req = String::from_utf8_lossy(&buf[..n]).into_owned();

    let code = code_from_request(&req).ok_or("callback carried no authorization code")?;

    let body = CALLBACK_HTML;
    let resp = format!("{HTTP_OK}Content-Length: {}\r\n\r\n{body}", body.len());
    let _ = conn.write_all(resp.as_bytes()).await;
    let _ = conn.shutdown().await;
    Ok(code)
}

fn code_from_request(req: &str) -> Option<String> {
    let path = req.split_whitespace().nth(1)?;
    let query = path.split_once('?')?.1;
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix(CODE_PREFIX))
        .map(str::to_owned)
}

/// `$CODEX_HOME` (default `$HOME/.codex`) — shared with the codex CLI.
pub fn codex_home() -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_code_from_callback_request() {
        let req = "GET /auth/callback?code=abc-123&state=x HTTP/1.1\r\nHost: localhost\r\n\r\n";
        assert_eq!(code_from_request(req), Some("abc-123".into()));
    }

    #[test]
    fn rejects_request_without_code() {
        let req = "GET /auth/callback?error=cancelled HTTP/1.1\r\n\r\n";
        assert_eq!(code_from_request(req), None);
    }
}
