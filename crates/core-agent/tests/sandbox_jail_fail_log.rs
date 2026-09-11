//! Integration test: a failing sandbox command must still be recorded in the
//! transcript (with its error), so run logs reach the backend and the web UI.

use core_agent::sandbox_jail::Sandbox;

fn assert_failure_logged() {
    let work = std::env::temp_dir().join("susutaku-sandbox-fail-log-test");
    let sb = Sandbox::new_in(&work).expect("new_in");

    let err = sb.run("echo boom >&2; exit 3").expect_err("run fails");
    let transcript = sb.transcript();
    let last = transcript
        .last()
        .map(|e| e.content.clone())
        .unwrap_or_default();

    assert!(last.contains("$ echo boom >&2; exit 3"), "missing cmd in {last:?}");
    assert!(last.contains("boom"), "missing stderr in {last:?}");
    assert!(last.contains("error"), "missing error marker in {last:?}");
    assert!(!err.to_string().is_empty());

    sb.purge();
    std::fs::remove_dir_all(&work).ok();
}

#[cfg(target_os = "linux")]
#[test]
fn failing_command_is_recorded() {
    assert_failure_logged();
}

#[cfg(target_os = "macos")]
#[test]
fn failing_command_is_recorded() {
    assert_failure_logged();
}
