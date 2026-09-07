use std::env;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const CODEX_BIN: &str = "codex";
const CODEX_BIN_ENV: &str = "CODEX_BIN";
const CODEX_HOME_ENV: &str = "CODEX_HOME";
const SKIP_GIT_CHECK: &str = "--skip-git-repo-check";
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
        }
    }
}

impl std::error::Error for ExecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ExecError::Spawn(e) => Some(e),
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
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(ExecError::Spawn)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_binary_reports_spawn_error() {
        let result = Command::new("codex-definitely-not-installed")
            .arg("--version")
            .status();
        assert!(result.is_err());
    }
}
