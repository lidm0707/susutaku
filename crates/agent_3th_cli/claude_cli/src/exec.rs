use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

const CLAUDE_BIN: &str = "claude";
const CLAUDE_HOME_ENV: &str = "CLAUDE_CONFIG_DIR";

#[derive(Debug)]
pub enum ExecError {
    Spawn(std::io::Error),
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecError::Spawn(e) => write!(f, "spawn `{CLAUDE_BIN}`: {e}"),
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

impl From<std::io::Error> for ExecError {
    fn from(e: std::io::Error) -> Self {
        ExecError::Spawn(e)
    }
}

/// Spawn `claude -p` with streaming JSON events under the sandbox `CLAUDE_CONFIG_DIR`.
pub fn exec_json(prompt: &str, claude_home: &Path) -> Result<Child, ExecError> {
    Command::new(CLAUDE_BIN)
        .args(["-p", prompt, "--output-format", "stream-json", "--verbose"])
        .env(CLAUDE_HOME_ENV, claude_home)
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

pub fn check_available() -> std::io::Result<()> {
    Command::new(CLAUDE_BIN)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(drop)
}
