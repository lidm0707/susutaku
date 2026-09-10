//! Integration test: real sandbox spawn on Linux (user/mount/pid/net ns +
//! chroot jail + seccomp). Runs a command, asserts real output and
//! workspace persistence across runs.
#![cfg(target_os = "linux")]

use core_agent::sandbox::Sandbox;

#[test]
fn sandbox_runs_command_and_persists_workspace() {
    let work = std::env::temp_dir().join("susutaku-sandbox-run-test");
    let sb = Sandbox::new_in(&work).expect("new_in");

    let out = sb.run("echo HELLO; id -u").expect("first run");
    assert!(out.contains("HELLO"), "missing stdout in {out:?}");

    // Second command sees files written by the first: workspace persists.
    sb.run("echo persisted > proof.txt").expect("write run");
    let out = sb.run("cat proof.txt").expect("read run");
    assert!(out.contains("persisted"), "missing proof in {out:?}");

    sb.purge();
    std::fs::remove_dir_all(&work).ok();
}
