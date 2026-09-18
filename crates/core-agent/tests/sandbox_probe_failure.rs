//! Integration test: a sandbox probe command that exits non-zero with
//! empty stderr must surface its stdout and the real exit code — podman
//! passes the container's exit code through, so this is the command
//! failing, not a missing podman/image.

use core_agent::podman::Sandbox;

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn failing_probe_reports_stdout_and_exit_code() {
    let work = std::env::temp_dir().join("susutaku-sandbox-probe-test");
    let sb = Sandbox::new_in(&work).expect("new_in");

    let err = sb
        .run("echo PROBE-STDOUT; ls /definitely-missing 2>/dev/null; exit 2")
        .expect_err("non-zero exit is an Err");
    let msg = err.to_string();

    assert!(msg.contains("PROBE-STDOUT"), "stdout lost: {msg:?}");
    assert!(msg.contains("exited with 2"), "exit code lost: {msg:?}");
    assert!(
        !msg.contains("missing `podman` or image"),
        "infra hint shown for a plain command failure: {msg:?}"
    );

    // the transcript records the run so the UI shows what happened
    let last = sb
        .transcript()
        .last()
        .map(|e| e.content.clone())
        .unwrap_or_default();
    assert!(
        last.contains("PROBE-STDOUT"),
        "transcript lost stdout: {last:?}"
    );

    sb.purge();
    std::fs::remove_dir_all(&work).ok();
}
