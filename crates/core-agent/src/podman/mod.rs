//! Podman sandbox: every command runs in a fresh rootless container
//! (`podman run --rm`), workspace bind-mounted read-write at `/workspace`,
//! fixed env allow-list, rlimit prelude, host-side timeout + output caps.
//!
//! # Podman vs the old native jail
//!
//! Before this module, each OS had a hand-rolled jail
//! (`sandbox_jail/{linux,macos,windows}.rs`, now removed): Linux used raw
//! `unshare` user/PID/mount/net namespaces + chroot + a seccomp BPF
//! deny-list; macOS used `sandbox-exec` (Seatbelt) profiles; Windows used
//! restricted-token processes + JobObjects. This module replaces all three
//! with one implementation that delegates the kernel-level isolation to
//! rootless Podman (crun: namespaces + seccomp + cgroups managed by the
//! OCI runtime).
//!
//! |                   | native jail (removed)         | podman (this module)              |
//! |-------------------|-------------------------------|-----------------------------------|
//! | isolation         | per-OS code (userns/chroot,   | OCI runtime, uniform on every OS  |
//! |                   | seatbelt, job objects)        |                                   |
//! | filesystem        | chroot with RO binds of       | image is the root; only the       |
//! |                   | /bin,/usr,/lib                | workspace bind is writable        |
//! | network           | netns, loopback only — no     | `--network=none` or Podman's      |
//! |                   | outbound path at all          | rootless net (slirp4netns, real   |
//! |                   |                               | outbound) with `Enabled`          |
//! | resources         | setrlimit before exec         | `--pids-limit` + shell `ulimit`   |
//! |                   |                               | prelude + host-side timeout       |
//! | toolchain         | whatever the host has         | pinned image                      |
//! | cost per command  | ~0 (fork/exec)                | container create (~100ms)         |
//!
//! Security invariants kept from the native jail: fixed env allow-list (no
//! host env leaks), never falls back to unsandboxed execution, host-side
//! timeout with `podman rm -f` so no orphan containers survive.
//!
//! # Prerequisites
//! - `podman` on PATH (rootless).
//! - Default image built: `make sandbox-image`
//!   (override with `SUSUTAKU_SANDBOX_IMAGE`).

pub mod install;
pub mod limits;
pub mod runner;
pub mod sandbox;
pub mod state;
pub mod util;

pub use limits::{NetworkPolicy, NetworkPolicyChoice, SandboxConfig, SandboxLimits};
pub use sandbox::{Role, Sandbox, run};
pub use state::SANDBOX_PREFIX;
pub use state::{SandboxDir, list_dirs, load_transcript, purge_dir, state_path};

pub(crate) const WORKSPACE_MOUNT: &str = "workspace";

pub fn run_in_sandbox(cmd: &str) -> Result<String, std::io::Error> {
    sandbox::run(cmd)
}

const COPY_DIR_COPY_ERR: &str = "snapshot copy failed";

/// Recursively copy `src` into `dst`, skipping `skip_name` entries at any
/// depth (used when dst lives inside src).
pub(crate) fn copy_dir_recursive_skip(
    src: &Path,
    dst: &Path,
    skip_name: &str,
) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy() == skip_name {
            continue;
        }
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive_skip(&entry.path(), &to, skip_name)?;
        } else {
            fs::copy(entry.path(), &to).map_err(|e| {
                std::io::Error::other(format!(
                    "{COPY_DIR_COPY_ERR}: {}: {e}",
                    entry.path().display()
                ))
            })?;
        }
    }
    Ok(())
}

use std::fs;
use std::path::Path;
