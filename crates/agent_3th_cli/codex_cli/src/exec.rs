use std::env;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const CODEX_BIN: &str = "codex";
const CODEX_BIN_ENV: &str = "CODEX_BIN";
const CODEX_HOME_ENV: &str = "CODEX_HOME";
const SKIP_GIT_CHECK: &str = "--skip-git-repo-check";
const LOGIN_CMD: &str = "login";
const AUTH_URL_MARKER: &str = "https://auth.openai.com";
/// Upper bound on stdout lines scanned for the authorize URL.
const LOGIN_URL_MAX_LINES: usize = 200;
/// Known install locations when `codex` is not on the backend's PATH.
const FALLBACK_BINS: &[&str] = &[
    "/opt/homebrew/bin/codex",
    "/usr/local/bin/codex",
    ".codex/plugins/.plugin-appserver/codex",
];

/// `$CODEX_BIN` → `codex` on PATH → known fallback locations.
pub fn resolve_bin() -> PathBuf {
    if let Ok(p) = env::var(CODEX_BIN_ENV) {
        let path = PathBuf::from(p);
        if path.exists() {
            return path;
        }
    }
    if Command::new(CODEX_BIN)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
    {
        return PathBuf::from(CODEX_BIN);
    }
    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    FALLBACK_BINS
        .iter()
        .map(|rel| {
            let p = PathBuf::from(rel);
            if p.is_absolute() { p } else { home.join(p) }
        })
        .find(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from(CODEX_BIN))
}

#[derive(Debug)]
pub enum ExecError {
    Spawn(std::io::Error),
    /// `codex login` produced no authorize URL on stdout/stderr.
    NoLoginUrl,
}

impl From<std::io::Error> for ExecError {
    fn from(e: std::io::Error) -> Self {
        ExecError::Spawn(e)
    }
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecError::Spawn(e) => write!(f, "spawn `{CODEX_BIN}`: {e}"),
            ExecError::NoLoginUrl => {
                write!(f, "`{CODEX_BIN} {LOGIN_CMD}` printed no authorize URL")
            }
        }
    }
}

impl std::error::Error for ExecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ExecError::Spawn(e) => Some(e),
            ExecError::NoLoginUrl => None,
        }
    }
}

/// Spawn `codex exec --json` inside `workspace` with the sandbox `CODEX_HOME`;
/// caller reads stdout lines.
pub fn exec_json(
    prompt: &str,
    model: Option<&str>,
    codex_home: &Path,
    workspace: &Path,
) -> Result<Child, ExecError> {
    let mut cmd = Command::new(resolve_bin());
    cmd.args(["exec", "--json", SKIP_GIT_CHECK]);
    if let Some(m) = model {
        cmd.args(["-m", m]);
    }
    cmd.arg(prompt)
        .current_dir(workspace)
        .env(CODEX_HOME_ENV, codex_home)
        // codex exec reads stdin alongside the prompt when it is not a TTY;
        // an inherited (never-closing) stdin would block it forever.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(ExecError::Spawn)
}

/// Spawn `codex login` with the given `CODEX_HOME`; returns the child plus
/// the authorize URL parsed from its output. The child stays running and
/// handles the OAuth callback itself; when it exits, `auth.json` is written.
/// The CLI prints the URL on stderr ("Starting local login server ...").
pub fn login(codex_home: &Path) -> Result<(Child, String), ExecError> {
    let mut child = Command::new(resolve_bin())
        .arg(LOGIN_CMD)
        .env(CODEX_HOME_ENV, codex_home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(ExecError::Spawn)?;
    let stderr = child.stderr.take().ok_or(ExecError::NoLoginUrl)?;
    let url = BufReader::new(stderr)
        .lines()
        .take(LOGIN_URL_MAX_LINES)
        .find_map(|line| login_url_from_line(&line.ok()?));
    match url {
        Some(url) => {
            // Keep draining stderr so the CLI never blocks on a full pipe.
            if let Some(mut rest) = child.stderr.take() {
                std::thread::spawn(move || std::io::copy(&mut rest, &mut std::io::sink()));
            }
            Ok((child, url))
        }
        None => {
            let _ = child.kill();
            let _ = child.wait();
            Err(ExecError::NoLoginUrl)
        }
    }
}

/// The authorize URL is whatever starts at the auth marker up to the first
/// whitespace or quote on the line (stdout or stderr may wrap it in prose).
fn login_url_from_line(line: &str) -> Option<String> {
    let start = line.find(AUTH_URL_MARKER)?;
    let rest = &line[start..];
    let end = rest.find([' ', '\t', '"', '\'']).unwrap_or(rest.len());
    Some(rest[..end].to_owned())
}

/// Blocking drain of the child's newline-delimited JSON events.
pub fn drain_events(child: &mut Child, mut on_event: impl FnMut(&str)) -> std::io::Result<()> {
    let Some(stdout) = child.stdout.take() else {
        return Ok(());
    };
    for line in BufReader::new(stdout).lines() {
        on_event(line?.as_str());
    }
    child.wait()?;
    Ok(())
}

pub fn check_available() -> Result<PathBuf, ExecError> {
    let bin = resolve_bin();
    Command::new(&bin)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|_| bin)
        .map_err(ExecError::from)
}
