//! Integration tests for the public installer API surface only.

use backend::infra::client::install;

#[test]
fn script_bakes_server() {
    let script = install::render_install_script("http://host:3334");
    assert!(script.contains("server=http://host:3334"));
}

#[test]
fn script_installs_runtime_worker_only() {
    let script = install::render_install_script("http://host:3334");
    assert!(script.contains("-p backend"));
    assert!(
        !script.contains("local-model"),
        "runtime install must not build or launch the model server"
    );
    assert!(script.contains(install::SERVER_URL_ENV));
    assert!(script.contains(install::HUB_ADDR_ENV));
    assert!(script.contains(install::CLIENT_RAM_ENV));
}

#[test]
fn script_checks_runtime_dependencies_per_os() {
    let script = install::render_install_script("http://host:3334");
    assert!(script.contains(install::SANDBOX_SHELL_MACOS));
    assert!(script.contains(install::SANDBOX_SHELL_LINUX));
    assert!(script.contains(install::SANDBOX_SHELL_WINDOWS));
    assert!(script.contains("missing runtime dependencies"));
    assert!(script.contains("windows runtime client is not supported yet"));
}

#[test]
fn script_passes_shell_syntax_check() {
    let script = install::render_install_script("http://host:3334");
    let path = std::env::temp_dir().join("susutaku-install-test.sh");
    std::fs::write(&path, script).unwrap();
    let status = std::process::Command::new("sh")
        .arg("-n")
        .arg(&path)
        .status()
        .expect("sh available");
    assert!(status.success(), "generated script must be valid POSIX sh");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn script_is_valid_posix_shell_shape() {
    let script = install::render_install_script("http://host:3334");
    assert!(script.starts_with("#!/bin/sh"));
    assert!(script.contains("set -eu"));
}
