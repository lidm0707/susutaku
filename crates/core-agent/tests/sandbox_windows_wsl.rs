//! WSL pass-through execution mode tests. Run on a Windows host with WSL
//! installed: `cargo test -p core-agent --target x86_64-pc-windows-msvc --test sandbox_windows_wsl`

#![cfg(windows)]

use core_agent::sandbox::windows as sb;
use core_agent::sandbox::windows::WindowsExec;

use std::path::Path;
use std::sync::RwLock;

static STATE_LOCK: RwLock<()> = RwLock::new(());

#[test]
fn detect_returns_a_mode() {
    let mode = sb::detect();
    assert!(mode == WindowsExec::RestrictedToken || mode == WindowsExec::Wsl);
    // Cached: repeated calls are stable.
    assert_eq!(sb::detect(), mode);
}

#[test]
fn detect_matches_wsl_status() {
    let wsl_ok = std::process::Command::new(sb::WSL_EXE)
        .arg(sb::WSL_STATUS_FLAG)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let expected = if wsl_ok {
        WindowsExec::Wsl
    } else {
        sb::DEFAULT_EXEC
    };
    assert_eq!(sb::detect(), expected);
}

#[test]
fn windows_path_to_wsl_translates_drive_paths() {
    assert_eq!(sb::windows_path_to_wsl(Path::new("C:\\a\\b")), "/mnt/c/a/b");
    assert_eq!(
        sb::windows_path_to_wsl(Path::new("d:\\x\\y.txt")),
        "/mnt/d/x/y.txt"
    );
}

#[test]
fn windows_path_to_wsl_passes_through_non_drive() {
    assert_eq!(sb::windows_path_to_wsl(Path::new("/mnt/c/a")), "/mnt/c/a");
    assert_eq!(
        sb::windows_path_to_wsl(Path::new("relative/path")),
        "relative/path"
    );
}

#[test]
fn wsl_mode_runs_via_sh() {
    let _g = STATE_LOCK.read().unwrap();
    if sb::detect() != WindowsExec::Wsl {
        eprintln!("skipping: WSL not available");
        return;
    }
    let sandbox = sb::Sandbox::new().unwrap();
    let out = sandbox.run("echo hi && pwd").unwrap();
    assert_eq!(out.trim().lines().next(), Some("hi"));
    let pwd = out.trim().lines().last().unwrap_or_default();
    let expected = sb::windows_path_to_wsl(&sandbox.root());
    assert_eq!(pwd, expected, "command must run in the workspace root");
    sandbox.purge();
}

#[test]
fn wsl_mode_timeout_still_enforced() {
    let _g = STATE_LOCK.read().unwrap();
    if sb::detect() != WindowsExec::Wsl {
        eprintln!("skipping: WSL not available");
        return;
    }
    let sandbox = sb::Sandbox::new().unwrap();
    let limits = sb::SandboxLimits {
        timeout_secs: 2,
        ..Default::default()
    };
    let result = sandbox.run_with_limits("sleep 30", &limits);
    assert!(result.is_err(), "long WSL command must hit the timeout");
    sandbox.purge();
}

#[test]
fn default_exec_is_restricted_token() {
    assert_eq!(sb::DEFAULT_EXEC, WindowsExec::RestrictedToken);
}
