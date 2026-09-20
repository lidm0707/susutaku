//! codex CLI inside the podman sandbox: the sandbox image carries the CLI
//! (pre-init), the host `$CODEX_HOME` is bind-mounted read-only and copied
//! into a writable per-run `CODEX_HOME` at container start, and the MCP
//! bridge binary is bind-mounted read-only so codex gets the susutaku
//! toolcalls. Network is enabled — codex must reach its model API.

use std::io::Error;
use std::path::PathBuf;
use std::time::Duration;

use super::limits::{NetworkPolicyChoice, SandboxLimits};
use super::sandbox::Sandbox;

const CODEX_BIN_IN: &str = "/usr/local/bin/codex";
const MCP_BIN_IN: &str = "/usr/local/bin/susutaku-mcp";
const AUTH_MOUNT: &str = "/opt/codex-auth";
const CODEX_HOME_IN: &str = "/tmp/codex-home";
const CODEX_HOME_ENV: &str = "CODEX_HOME";
const AUTH_FILE: &str = "auth.json";
const CONFIG_FILE: &str = "config.toml";
const HEREDOC_EOF: &str = "SUSUTAKU_MCP_EOF";
/// codex exec streams a full agent loop — far above the plain tool timeout.
const CODEX_TIMEOUT_SECS: u64 = 15 * 60;
const CODEX_MAX_PROCESSES: u64 = 512;

/// Host-side inputs for a sandboxed codex run. Host paths live on the
/// backend's filesystem (bind-mounted into the container read-only).
pub struct CodexEnv {
    /// Host `$CODEX_HOME` holding `auth.json` (codex OAuth tokens).
    pub codex_home: PathBuf,
    /// Host path of the `susutaku-mcp` MCP bridge binary.
    pub mcp_bin: PathBuf,
    /// MCP servers TOML block written into the sandbox `config.toml`
    /// (built by `codex_cli::mcp::server_block` with [`MCP_BIN_IN`]).
    pub mcp_block: String,
}

impl CodexEnv {
    /// Read-only bind mounts: auth home + MCP bridge binary.
    pub fn mounts(&self) -> Vec<String> {
        vec![
            format!("{}:{AUTH_MOUNT}:ro", self.codex_home.display()),
            format!("{}:{MCP_BIN_IN}:ro", self.mcp_bin.display()),
        ]
    }
}

/// Sandbox limits for a codex agent run: long timeout, generous process
/// budget (node/codex spawn helpers), otherwise the defaults.
pub fn limits() -> SandboxLimits {
    SandboxLimits {
        timeout: Duration::from_secs(CODEX_TIMEOUT_SECS),
        max_processes: CODEX_MAX_PROCESSES,
        ..SandboxLimits::default()
    }
}

/// Preinit prelude: seed a writable per-run `CODEX_HOME` with the OAuth
/// tokens (when present) and the MCP config, then run codex.
pub fn prelude(env: &CodexEnv) -> String {
    let auth = if env.codex_home.join(AUTH_FILE).exists() {
        format!("cp {AUTH_MOUNT}/{AUTH_FILE} {CODEX_HOME_IN}/{AUTH_FILE} && ")
    } else {
        String::new()
    };
    format!(
        "mkdir -p {CODEX_HOME_IN} && {auth}cat > {CODEX_HOME_IN}/{CONFIG_FILE} <<'{HEREDOC_EOF}'\n{}\n{HEREDOC_EOF}\n",
        env.mcp_block.trim()
    )
}

/// The `codex exec --json` invocation (shell-quoted prompt/model).
pub fn exec_cmd(prompt: &str, model: Option<&str>) -> String {
    let model = model
        .filter(|m| !m.is_empty())
        .map(|m| format!("-m {} ", shell_quote(m)))
        .unwrap_or_default();
    format!(
        "{CODEX_BIN_IN} exec --json --skip-git-repo-check {model}{}",
        shell_quote(prompt)
    )
}

/// Run one codex exec inside `sandbox`: preinit prelude + exec, network
/// enabled, `CODEX_HOME` pointed at the seeded dir.
pub fn run(
    sandbox: &Sandbox,
    env: &CodexEnv,
    prompt: &str,
    model: Option<&str>,
) -> Result<String, Error> {
    let cmd = format!("{}{}", prelude(env), exec_cmd(prompt, model));
    sandbox.run_with_env_mounts(
        &cmd,
        &limits(),
        NetworkPolicyChoice::Enabled,
        &[(CODEX_HOME_ENV.to_string(), CODEX_HOME_IN.to_string())],
        &env.mounts(),
    )
}

/// POSIX single-quote shell escaping.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}
