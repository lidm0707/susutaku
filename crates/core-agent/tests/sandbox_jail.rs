//! Sandbox integration tests, portable across macOS/Linux/Windows via `sb`.

use core_agent::podman as sb;
use core_agent::podman::Role;

use std::sync::Mutex;

static STATE_LOCK: Mutex<()> = Mutex::new(());

/// The instance state dir always exists while the process runs; what matters
/// is how many per-sandbox state files it holds.
fn state_json_count() -> usize {
    std::fs::read_dir(sb::state_path())
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
                .count()
        })
        .unwrap_or(0)
}

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
    assert_eq!(state_json_count(), 1);
    let t = sandbox.transcript();
    assert_eq!(t.len(), 1);
    drop(sandbox);
    assert_eq!(state_json_count(), 1);
    sb::Sandbox::purge_stale();
    assert_eq!(state_json_count(), 0);
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
    assert_eq!(state_json_count(), 0);
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
