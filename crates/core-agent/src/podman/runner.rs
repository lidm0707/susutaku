//! Container execution: builds the `podman run` invocation, streams output
//! with a host-side deadline, and force-removes the container on overrun.

use std::io::Error;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::limits::{NetworkPolicyChoice, SandboxLimits};
use super::util::{cwd_mount_path, drain, truncate_bytes};

const PODMAN_BIN: &str = "podman";
const DEFAULT_IMAGE: &str = "localhost/susutaku-sandbox:latest";
const IMAGE_ENV: &str = "SUSUTAKU_SANDBOX_IMAGE";
const RUN_NAME_PREFIX: &str = "susutaku-run";
const CONTAINER_WORKSPACE: &str = "/workspace";
const NOFILE: u64 = 64;
const STACK_KB: u64 = 8 * 1024;
const ERR_REPORT_CAP: usize = 4096;
const POLL_INTERVAL_MS: u64 = 20;

pub(crate) const SHELL: &str = "/bin/bash";
pub(crate) const RUN_FLAG: &str = "-c";

/// Fixed environment passed into the container — host env never leaks.
const SANDBOX_ENV: [(&str, &str); 5] = [
    ("PATH", "/usr/bin:/bin:/usr/sbin:/sbin"),
    ("HOME", "/workspace"),
    ("USER", "sandbox"),
    ("TMPDIR", "/tmp"),
    ("LANG", "C.UTF-8"),
];

/// Run one command in a fresh `--rm` container. `workspace` is the host-side
/// bind source, `cwd_rel` the stored relative cwd, `seq` disambiguates
/// concurrent container names for the same sandbox.
pub(crate) fn run_container(
    sandbox_id: &str,
    seq: u64,
    workspace: &Path,
    cwd_rel: &str,
    cmd: &str,
    limits: &SandboxLimits,
    network: NetworkPolicyChoice,
) -> Result<String, Error> {
    super::install::ensure_podman()?;
    let workspace = std::fs::canonicalize(workspace)?;
    let name = format!("{RUN_NAME_PREFIX}-{sandbox_id}-{seq}");

    let mut command = Command::new(PODMAN_BIN);
    command
        .arg("run")
        .arg("--rm")
        .arg(format!("--name={name}"))
        .arg("--pids-limit")
        .arg(limits.max_processes.to_string());
    match network {
        NetworkPolicyChoice::Disabled => {
            command.arg("--network=none");
        }
        NetworkPolicyChoice::Enabled => {}
    }
    command
        .arg("-v")
        .arg(format!("{}:{CONTAINER_WORKSPACE}", workspace.display()));
    command.arg("-w").arg(cwd_mount_path(cwd_rel));
    for (k, v) in SANDBOX_ENV {
        command.arg("-e").arg(format!("{k}={v}"));
    }
    command.arg(image());
    command.arg(SHELL).arg(RUN_FLAG);
    command.arg(format!("{}{cmd}", ulimit_prelude(limits)));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn()?;
    let mut out = child
        .stdout
        .take()
        .ok_or_else(|| Error::other("podman stdout unavailable"))?;
    let mut err = child
        .stderr
        .take()
        .ok_or_else(|| Error::other("podman stderr unavailable"))?;
    let mut out_buf = Vec::new();
    let mut err_buf = Vec::new();
    let max_out = limits.max_output_bytes;
    let deadline = Instant::now() + limits.timeout;
    loop {
        drain(&mut out, max_out, &mut out_buf);
        drain(&mut err, max_out, &mut err_buf);
        match child.try_wait()? {
            Some(_) => break,
            None if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            }
            None => {
                let _ = child.kill();
                let _ = child.wait();
                force_remove(&name);
                return Err(Error::other(format!(
                    "sandbox command timed out after {}s",
                    limits.timeout.as_secs()
                )));
            }
        }
    }
    drain(&mut out, max_out, &mut out_buf);
    drain(&mut err, max_out, &mut err_buf);
    let status = child.wait()?;

    if !status.success() {
        let mut msg = String::from_utf8_lossy(&err_buf).into_owned();
        truncate_bytes(&mut msg, ERR_REPORT_CAP);
        if msg.is_empty() {
            msg = format!(
                "podman exited with {status} (missing `{PODMAN_BIN}` or image `{}`?)",
                image()
            );
        }
        return Err(Error::other(msg));
    }
    let mut text = String::from_utf8_lossy(&out_buf).into_owned();
    truncate_bytes(&mut text, limits.max_output_bytes);
    Ok(text)
}

/// Shell rlimit prelude applied before each command (bash builtins).
fn ulimit_prelude(limits: &SandboxLimits) -> String {
    let mut pre = format!(
        "ulimit -u {}; ulimit -n {NOFILE}; ulimit -s {STACK_KB}; ",
        limits.max_processes.max(1)
    );
    if let Some(mem) = limits.max_memory_bytes {
        pre.push_str(&format!("ulimit -v {}; ", mem / 1024));
    }
    if let Some(cpu) = limits.max_cpu_seconds {
        pre.push_str(&format!("ulimit -t {cpu}; "));
    }
    if let Some(fsz) = limits.max_file_size_bytes {
        pre.push_str(&format!("ulimit -f {}; ", fsz / 1024));
    }
    pre
}

fn image() -> String {
    std::env::var(IMAGE_ENV).unwrap_or_else(|_| DEFAULT_IMAGE.to_owned())
}

fn force_remove(name: &str) {
    let _ = Command::new(PODMAN_BIN)
        .args(["rm", "-f", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
