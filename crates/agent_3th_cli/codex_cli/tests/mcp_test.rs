#![allow(clippy::unwrap_used)] // tests: unwrap is the assertion tool

use codex_cli::mcp::{ensure_config, resolve_mcp_bin};
use std::fs;

#[test]
fn config_appends_server_block_once() {
    let dir = std::env::temp_dir().join(format!("codex-mcp-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let bin = dir.join("susutaku-mcp");
    fs::write(&bin, "#!/bin/sh\n").unwrap();

    ensure_config(&dir, &bin);
    let first = fs::read_to_string(dir.join("config.toml")).unwrap();
    assert!(first.contains("[mcp_servers.susutaku]"));
    assert!(first.contains(&format!("command = \"{}\"", bin.display())));

    // Idempotent: a second pass must not duplicate the section.
    ensure_config(&dir, &bin);
    let second = fs::read_to_string(dir.join("config.toml")).unwrap();
    assert_eq!(second.matches("[mcp_servers.susutaku]").count(), 1);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn resolve_bin_prefers_env_override() {
    let dir = std::env::temp_dir().join(format!("codex-mcp-env-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let bin = dir.join("my-mcp");
    fs::write(&bin, "").unwrap();
    // SAFETY: single-threaded test process; no other code reads this var.
    unsafe { std::env::set_var("SUSUTAKU_MCP_BIN", &bin) };
    assert_eq!(resolve_mcp_bin(), Some(bin.clone()));
    unsafe { std::env::remove_var("SUSUTAKU_MCP_BIN") };
    fs::remove_dir_all(&dir).unwrap();
}
