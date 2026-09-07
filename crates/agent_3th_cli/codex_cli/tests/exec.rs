use std::process::Command;

#[test]
fn missing_binary_reports_spawn_error() {
    let result = Command::new("codex-definitely-not-installed")
        .arg("--version")
        .status();
    assert!(result.is_err());
}
