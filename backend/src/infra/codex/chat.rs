//! Chat through the codex CLI (`codex exec --json`) using the shared login.
//! The CLI runs inside the podman sandbox (image pre-init carries the CLI;
//! auth + MCP config are seeded at container start — see
//! `core_agent::podman::codex`).

use core_agent::podman::codex::CodexEnv;
use core_agent::podman::{Sandbox, codex as sandbox_codex};
use serde_json::Value;
use std::path::Path;

use crate::infra::codex::auth::codex_home;

const AGENT_EVENT: &str = "agent_message";
const ERROR_EVENT: &str = "error";
const MCP_BIN_IN: &str = "/usr/local/bin/susutaku-mcp";

/// Run one prompt through codex inside a podman sandbox rooted at
/// `work_tree`, return the final agent text.
pub fn chat(prompt: &str, model: Option<&str>, work_tree: &Path) -> Result<String, String> {
    let mcp_bin = codex_cli::mcp::resolve_mcp_bin()
        .ok_or("susutaku-mcp bridge not found next to the backend; codex would run tool-less")?;
    let env = CodexEnv {
        codex_home: codex_home(),
        mcp_bin,
        mcp_block: codex_cli::mcp::server_block(MCP_BIN_IN),
    };
    let sandbox = Sandbox::new_in(work_tree).map_err(|e| format!("codex sandbox failed: {e}"))?;
    let out = sandbox_codex::run(&sandbox, &env, prompt, model)
        .map_err(|e| format!("codex sandbox exec failed: {e}"));
    sandbox.purge();
    let out = out?;
    let mut text = String::new();
    let mut err = String::new();
    for line in out.lines() {
        extract(line, &mut text, &mut err);
    }
    if text.is_empty() {
        return Err(if err.is_empty() {
            "codex returned no reply".to_string()
        } else {
            err
        });
    }
    Ok(text)
}

/// Collect agent messages and error payloads from the codex JSONL event stream.
pub fn extract(line: &str, text: &mut String, err: &mut String) {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return;
    };
    if let Some(m) = v.get("msg") {
        if m.get("type").and_then(Value::as_str) == Some(AGENT_EVENT) {
            push_str(text, m.get("message"));
        }
        if m.get("type").and_then(Value::as_str) == Some(ERROR_EVENT) {
            push_str(err, m.get("message"));
        }
    }
    if let Some(item) = v.get("item")
        && item.get("type").and_then(Value::as_str) == Some(AGENT_EVENT)
    {
        push_str(text, item.get("text"));
    }
}

fn push_str(into: &mut String, v: Option<&Value>) {
    if let Some(s) = v.and_then(Value::as_str) {
        into.push_str(s);
    }
}
