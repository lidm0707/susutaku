//! First-use podman bootstrap: if the `podman` binary is missing on the
//! host, install it with the platform package manager. Runs once per
//! process; every failure is a hard error (the sandbox never runs
//! unsandboxed as a fallback).

use std::io::Error;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

const PODMAN_BIN: &str = "podman";

#[cfg(target_os = "linux")]
const APT_UPDATE: [&str; 2] = ["update", "-y"];
#[cfg(target_os = "linux")]
const APT_INSTALL: [&str; 3] = ["install", "-y", PODMAN_BIN];
#[cfg(target_os = "linux")]
const DNF_INSTALL: [&str; 3] = ["install", "-y", PODMAN_BIN];
#[cfg(target_os = "linux")]
const APK_INSTALL: [&str; 3] = ["add", "--no-cache", PODMAN_BIN];
#[cfg(target_os = "macos")]
const BREW_INSTALL: [&str; 2] = ["install", PODMAN_BIN];

/// Ensure `podman` is usable; installs it on first call if missing.
pub(crate) fn ensure_podman() -> Result<(), Error> {
    static DONE: OnceLock<()> = OnceLock::new();
    if DONE.get().is_some() {
        return Ok(());
    }
    if podman_works() {
        let _ = DONE.set(());
        return Ok(());
    }
    install()?;
    if !podman_works() {
        return Err(Error::other(
            "podman was installed but is still not runnable",
        ));
    }
    let _ = DONE.set(());
    Ok(())
}

fn podman_works() -> bool {
    Command::new(PODMAN_BIN)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn install() -> Result<(), Error> {
    #[cfg(target_os = "linux")]
    let ok = install_linux();
    #[cfg(target_os = "macos")]
    let ok = run("brew", &BREW_INSTALL);
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let ok = false;
    if ok {
        Ok(())
    } else {
        Err(Error::other(
            "podman is missing and could not be installed automatically; install it manually",
        ))
    }
}

#[cfg(target_os = "linux")]
fn install_linux() -> bool {
    if run("apt-get", &APT_UPDATE) && run("apt-get", &APT_INSTALL) {
        return true;
    }
    if run("dnf", &DNF_INSTALL) {
        return true;
    }
    run("apk", &APK_INSTALL)
}

fn run(bin: &str, args: &[&str]) -> bool {
    Command::new(bin)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}
