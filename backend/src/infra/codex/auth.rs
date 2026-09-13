//! Codex OAuth login orchestration: loopback callback listener + state.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const CALLBACK_PORT: u16 = 1455;
const LOGIN_TIMEOUT: Duration = Duration::from_secs(600);
const WATCH_POLL_MS: u64 = 50;
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
    /// Pending `codex login` child, kept so a new start can cancel it
    /// (the CLI owns callback port 1455 and its OAuth state).
    child: RwLock<Option<std::process::Child>>,
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
            child: RwLock::new(None),
            workspace,
        }
    }

    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// Begin a login. Prefers delegating to the real `codex login` (the CLI
    /// owns the OAuth flow and writes `auth.json` itself); falls back to the
    /// hand-rolled PKCE flow when the CLI is not installed. Returns the
    /// authorize URL for the browser.
    pub async fn start(self: &Arc<Self>) -> Result<String, String> {
        eprintln!(
            "[codex-auth] start: phase={:?}",
            self.phase.read().unwrap_or_else(|e| e.into_inner())
        );
        {
            let mut phase = self.phase.write().map_err(|_| "login state poisoned")?;
            if matches!(&*phase, LoginPhase::Awaiting) {
                // Cancel the stale login: its listener on 1455 would swallow
                // the new callback (state mismatch) and wedge the UI polling.
                eprintln!("[codex-auth] cancelling stale login child");
                self.cancel_child();
                *phase = LoginPhase::Idle;
            }
        }
        match self.begin_login().await {
            Ok(url) => {
                let mut phase = self.phase.write().map_err(|_| "login state poisoned")?;
                *phase = LoginPhase::Awaiting;
                eprintln!("[codex-auth] awaiting login, authorize url issued");
                Ok(url)
            }
            Err(e) => {
                eprintln!("[codex-auth] start failed: {e}");
                let mut phase = self.phase.write().map_err(|_| "login state poisoned")?;
                *phase = LoginPhase::Failed(e.clone());
                Err(e)
            }
        }
    }

    fn cancel_child(&self) {
        if let Some(mut child) = self.child.write().unwrap_or_else(|e| e.into_inner()).take() {
            let killed = child.kill().is_ok();
            let waited = child
                .wait()
                .map(|s| s.to_string())
                .unwrap_or_else(|e| e.to_string());
            eprintln!("[codex-auth] stale child killed={killed} wait={waited}");
        }
    }

    /// Spawn the actual login flow (CLI-delegated when available) and return
    /// the authorize URL. The phase lock is not held across the awaits here.
    async fn begin_login(self: &Arc<Self>) -> Result<String, String> {
        if codex_cli::check_available().is_ok() {
            let home = codex_home();
            eprintln!(
                "[codex-auth] cli available, spawning codex login (home={})",
                home.display()
            );
            let spawned = tokio::task::spawn_blocking(move || codex_cli::login(&home))
                .await
                .map_err(|e| e.to_string())?;
            let (child, url) = spawned.map_err(|e| e.to_string())?;
            *self.child.write().unwrap_or_else(|e| e.into_inner()) = Some(child);
            tokio::spawn(self.clone().watch());
            return Ok(url);
        }
        eprintln!("[codex-auth] cli missing, using fallback pkce flow");
        let verifier = codex_cli::new_verifier();
        let url = codex_cli::authorize_url(&verifier).map_err(|e| e.to_string())?;
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

    /// Poll the pending `codex login` until it exits (auth.json written →
    /// idle) or is cancelled by a newer start (no phase change). Blocking
    /// `wait()` is avoided so the child stays killable from `cancel_child`.
    async fn watch(self: Arc<Self>) {
        loop {
            tokio::time::sleep(Duration::from_millis(WATCH_POLL_MS)).await;
            let status = match self
                .child
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .as_mut()
            {
                // Cancelled by a newer start — it owns the phase now.
                None => return,
                Some(child) => child.try_wait(),
            };
            match status {
                // Still running — keep polling.
                Ok(None) => {}
                Ok(Some(s)) if s.success() => {
                    eprintln!("[codex-auth] login child exited ok");
                    self.finish(LoginPhase::Idle);
                    return;
                }
                Ok(Some(s)) => {
                    // The CLI can exit non-zero after it has already written
                    // auth.json (observed: status 101 post-exchange), so trust
                    // the credential file over the exit code.
                    if codex_cli::check(&codex_home()) == codex_cli::AuthStatus::LoggedIn {
                        eprintln!(
                            "[codex-auth] login child exited with {s}, but auth.json is valid"
                        );
                        self.finish(LoginPhase::Idle);
                    } else {
                        eprintln!("[codex-auth] login child failed: {s}");
                        self.finish(LoginPhase::Failed(format!("codex login exited with {s}")));
                    }
                }
                Err(e) => {
                    eprintln!("[codex-auth] login child wait error: {e}");
                    self.finish(LoginPhase::Failed(e.to_string()));
                    return;
                }
            }
        }
    }

    async fn run(self: Arc<Self>, verifier: String) {
        eprintln!("[codex-auth] fallback flow waiting on callback port {CALLBACK_PORT}");
        let code = match callback_code().await {
            Ok(code) => code,
            Err(e) => {
                eprintln!("[codex-auth] fallback callback error: {e}");
                self.finish(LoginPhase::Failed(e));
                return;
            }
        };
        let home = codex_home();
        let res =
            tokio::task::spawn_blocking(move || codex_cli::login_callback(&code, &verifier, &home))
                .await;
        match res {
            Ok(Ok(_)) => {
                eprintln!("[codex-auth] fallback token exchange ok");
                self.finish(LoginPhase::Idle);
            }
            Ok(Err(e)) => {
                eprintln!("[codex-auth] fallback token exchange failed: {e}");
                self.finish(LoginPhase::Failed(e.to_string()));
            }
            Err(e) => {
                eprintln!("[codex-auth] fallback exchange task error: {e}");
                self.finish(LoginPhase::Failed(e.to_string()));
            }
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

pub fn code_from_request(req: &str) -> Option<String> {
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
