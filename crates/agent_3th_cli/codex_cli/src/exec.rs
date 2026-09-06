use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const CODEX_BIN: &str = "codex";
const CODEX_HOME_ENV: &str = "CODEX_HOME";

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

/// Spawn `codex exec --json` with the sandbox `CODEX_HOME`; caller reads stdout lines.
pub fn exec_json(prompt: &str, codex_home: &Path) -> Result<Child, ExecError> {
    Command::new(CODEX_BIN)
        .args(["exec", "--json", prompt])
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
    Command::new(CODEX_BIN)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|_| PathBuf::from(CODEX_BIN))
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
