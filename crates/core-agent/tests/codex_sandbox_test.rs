#![allow(clippy::unwrap_used)] // tests: unwrap is the assertion tool

use core_agent::podman::codex::{CodexEnv, exec_cmd, limits, prelude};
use std::path::PathBuf;
use std::time::Duration;

fn env(auth: bool) -> CodexEnv {
    let home = std::env::temp_dir().join(format!("codex-sb-test-{}-{auth}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    if auth {
        std::fs::write(home.join("auth.json"), "{}").unwrap();
    }
    CodexEnv {
        codex_home: home,
        mcp_bin: PathBuf::from("/host/bin/susutaku-mcp"),
        mcp_block:
            "\n[mcp_servers.susutaku]\ncommand = \"/usr/local/bin/susutaku-mcp\"\nargs = []\n"
                .to_string(),
    }
}

#[test]
fn mounts_bind_auth_and_bridge_read_only() {
    let m = env(true).mounts();
    assert!(m[0].ends_with(":/opt/codex-auth:ro"));
    assert!(m[0].split(':').next().unwrap().len() > 1);
    assert_eq!(
        m[1],
        "/host/bin/susutaku-mcp:/usr/local/bin/susutaku-mcp:ro"
    );
}

#[test]
fn prelude_seeds_auth_and_config() {
    let p = prelude(&env(true));
    assert!(p.contains("cp /opt/codex-auth/auth.json /tmp/codex-home/auth.json && "));
    assert!(p.contains("cat > /tmp/codex-home/config.toml <<'SUSUTAKU_MCP_EOF'"));
    assert!(p.contains("[mcp_servers.susutaku]"));
    assert!(p.ends_with("SUSUTAKU_MCP_EOF\n"));
}

#[test]
fn prelude_skips_missing_auth() {
    let p = prelude(&env(false));
    assert!(!p.contains("auth.json"));
    assert!(p.contains("config.toml"));
}

#[test]
fn exec_cmd_quotes_prompt_and_model() {
    let cmd = exec_cmd("hi 'there'", Some("gpt-5"));
    assert!(cmd.contains("-m 'gpt-5' "));
    assert!(cmd.ends_with("'hi '\\''there'\\'''"));
    assert!(cmd.contains("exec --json --skip-git-repo-check"));
}

#[test]
fn exec_cmd_skips_empty_model() {
    assert!(!exec_cmd("hi", Some("")).contains("-m"));
    assert!(!exec_cmd("hi", None).contains("-m"));
}

#[test]
fn codex_limits_allow_long_runs() {
    let l = limits();
    assert!(l.timeout > Duration::from_secs(60));
    assert!(l.max_processes > 64);
}
