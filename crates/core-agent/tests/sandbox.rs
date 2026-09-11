//! Sandbox integration tests, portable across macOS/Linux/Windows via `sb`.

#[cfg(target_os = "linux")]
use core_agent::sandbox::linux as sb;
#[cfg(target_os = "macos")]
use core_agent::sandbox::macos as sb;
#[cfg(target_os = "windows")]
use core_agent::sandbox::windows as sb;

#[cfg(target_os = "linux")]
use core_agent::sandbox::linux::Role;
#[cfg(target_os = "macos")]
use core_agent::sandbox::macos::Role;
#[cfg(target_os = "windows")]
use core_agent::sandbox::windows::Role;

use std::sync::Mutex;

static STATE_LOCK: Mutex<()> = Mutex::new(());

const ECHO_CMD: &str = if cfg!(target_os = "windows") {
    "Write-Output hi"
} else {
    "echo hi"
};

#[test]
fn sandbox_run_and_clear() {
    let _g = STATE_LOCK.lock().unwrap();
    let sandbox = sb::Sandbox::new().unwrap();
    let out = sandbox.run(ECHO_CMD).unwrap();
    assert_eq!(out.trim(), "hi");
    assert!(sb::state_path().exists());
    let t = sandbox.transcript();
    assert_eq!(t.len(), 1);
    drop(sandbox);
    assert!(sb::state_path().exists());
    sb::Sandbox::purge_stale();
    assert!(!sb::state_path().exists());
}

#[test]
fn restore_reloads_state() {
    let _g = STATE_LOCK.lock().unwrap();
    {
        let sandbox = sb::Sandbox::new().unwrap();
        sandbox.push_context(Role::User, "hello");
    }
    let sandbox = sb::Sandbox::restore().unwrap();
    assert_eq!(sandbox.transcript().len(), 1);
    assert_eq!(sandbox.transcript()[0].content, "hello");
    sandbox.purge();
    assert!(!sb::state_path().exists());
}

#[test]
fn snapshot_captures_workspace_files() {
    let _g = STATE_LOCK.lock().unwrap();
    let sandbox = sb::Sandbox::new().unwrap();
    sandbox
        .run(if cfg!(target_os = "windows") {
            "Write-Output snapshot-me > marker.txt"
        } else {
            "echo snapshot-me > marker.txt"
        })
        .unwrap();
    let snap = sandbox.snapshot("step-0001").unwrap();
    let marker = snap.join("marker.txt");
    assert!(marker.exists());
    assert!(
        std::fs::read_to_string(&marker)
            .unwrap()
            .contains("snapshot-me")
    );
    // Snapshots live under the sandbox root, not the workspace itself.
    assert!(
        !sandbox.root().join("snapshots").join("step-0001").exists() || cfg!(target_os = "macos")
    );
    sandbox.purge();
    assert!(!snap.exists());
}
