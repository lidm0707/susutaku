//! Integration tests for the public installer API surface only.

use backend::infra::install::{self, Capability, Role};

const GIB: usize = 1024 * 1024 * 1024;

#[test]
fn classify_needs_apple_silicon_and_ram() {
    assert_eq!(
        install::classify("macos", "aarch64", 48 * GIB),
        Capability::Model
    );
    assert_eq!(
        install::classify("macos", "aarch64", 16 * GIB),
        Capability::WorkerOnly
    );
    assert_eq!(
        install::classify("macos", "x86_64", 64 * GIB),
        Capability::WorkerOnly
    );
    assert_eq!(
        install::classify("linux", "aarch64", 64 * GIB),
        Capability::WorkerOnly
    );
}

#[test]
fn parse_role_accepts_known_names() {
    assert_eq!(install::parse_role("auto"), Some(Role::Auto));
    assert_eq!(install::parse_role("model"), Some(Role::Model));
    assert_eq!(install::parse_role("worker"), Some(Role::Worker));
    assert_eq!(install::parse_role("nope"), None);
}

#[test]
fn script_bakes_role_and_server() {
    let script = install::render_install_script(Role::Worker, "http://host:3334");
    assert!(script.contains("server=http://host:3334"));
    assert!(script.contains("role=worker"));
    let script = install::render_install_script(Role::Auto, "http://host:3334");
    assert!(script.contains("role=auto"));
}

#[test]
fn script_model_role_enforces_probe() {
    let script = install::render_install_script(Role::Model, "http://host:3334");
    assert!(
        script.contains("cannot host a local model"),
        "model role must refuse incapable machines"
    );
}

#[test]
fn script_checks_sandbox_dependencies_per_os() {
    let script = install::render_install_script(Role::Auto, "http://host:3334");
    assert!(script.contains(install::SANDBOX_SHELL_MACOS));
    assert!(script.contains(install::SANDBOX_SHELL_LINUX));
    assert!(script.contains(install::SANDBOX_SHELL_WINDOWS));
    assert!(script.contains("missing sandbox dependencies"));
    assert!(script.contains("windows sandbox client is not supported yet"));
}

#[test]
fn script_passes_shell_syntax_check() {
    let script = install::render_install_script(Role::Auto, "http://host:3334");
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
    let script = install::render_install_script(Role::Auto, "http://host:3334");
    assert!(script.starts_with("#!/bin/sh"));
    assert!(script.contains("set -eu"));
}
