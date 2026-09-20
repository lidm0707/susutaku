//! MCP bridge wiring: point codex at the susutaku MCP stdio server
//! (`susutaku-mcp`) via `$CODEX_HOME/config.toml`, so `codex exec` gains
//! the platform toolcalls (fetch, web_search, definition, create_card).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const MCP_BIN_ENV: &str = "SUSUTAKU_MCP_BIN";
const MCP_BIN_NAME: &str = "susutaku-mcp";
const CONFIG_FILE: &str = "config.toml";
const SERVER_SECTION: &str = "[mcp_servers.susutaku]";
const COMMAND_KEY: &str = "command";

/// Locate the bridge binary: `$SUSUTAKU_MCP_BIN`, else a `susutaku-mcp`
/// sibling of the current executable. `None` → skip config injection.
pub fn resolve_mcp_bin() -> Option<PathBuf> {
    if let Ok(p) = env::var(MCP_BIN_ENV) {
        let path = PathBuf::from(p);
        if path.exists() {
            return Some(path);
        }
    }
    let exe = env::current_exe().ok()?;
    let sibling = exe.parent()?.join(MCP_BIN_NAME);
    sibling.exists().then_some(sibling)
}

/// Append the `[mcp_servers.susutaku]` block to `codex_home/config.toml`
/// when the bridge binary exists and the section is not already present.
/// Best-effort: a failed write must never break `codex exec`.
pub fn ensure_config(codex_home: &Path, bin: &Path) {
    if fs::create_dir_all(codex_home).is_err() {
        return;
    }
    let path = codex_home.join(CONFIG_FILE);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    if existing.contains(SERVER_SECTION) {
        return;
    }
    let block = format!(
        "\n{SERVER_SECTION}\n{COMMAND_KEY} = \"{}\"\nargs = []\n",
        escape(bin.to_string_lossy().as_ref())
    );
    let _ = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, block.as_bytes()));
}

/// Basic TOML basic-string escaping.
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Resolve the bridge binary and inject the codex MCP config; logs why it
/// was skipped instead of failing the spawn.
pub fn wire(codex_home: &Path) {
    match resolve_mcp_bin() {
        Some(bin) => ensure_config(codex_home, &bin),
        None => eprintln!("[mcp] {MCP_BIN_NAME} not found, codex runs without susutaku tools"),
    }
}
