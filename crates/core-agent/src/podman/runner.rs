//! Container execution: builds the `podman run` invocation, streams output
//! with a host-side deadline, and force-removes the container on overrun.

use std::fs;
use std::io::Error;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::limits::{NetworkPolicyChoice, SandboxLimits};
use super::util::{cwd_mount_path, drain, truncate_bytes};

const PODMAN_BIN: &str = "podman";
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

/// Container identity for one run: sandbox name parts, base image, the
/// optional cache tag the container is committed into after the run, and
/// extra read-only bind mounts (`host:container:ro` strings).
pub(crate) struct ContainerSpec<'a> {
    pub sandbox_id: &'a str,
    pub seq: u64,
    pub image: &'a str,
    pub cache_tag: Option<&'a str>,
    pub extra_mounts: &'a [String],
}

/// Run one command in a fresh container. `workspace` is the host-side
/// bind source, `cwd_rel` the stored relative cwd. With `cache_tag`, the
/// container is committed into that image after the run (instead of
/// `--rm`), so installs persist for the next spawn.
pub(crate) fn run_container(
    workspace: &Path,
    cwd_rel: &str,
    cmd: &str,
    limits: &SandboxLimits,
    network: NetworkPolicyChoice,
    spec: &ContainerSpec<'_>,
    extra_env: &[(String, String)],
) -> Result<String, Error> {
    super::install::ensure_podman()?;
    let workspace = std::fs::canonicalize(workspace)?;
    let name = format!("{RUN_NAME_PREFIX}-{}-{}", spec.sandbox_id, spec.seq);

    let mut command = Command::new(PODMAN_BIN);
    command.arg("run").arg(format!("--name={name}"));
    if spec.cache_tag.is_none() {
        command.arg("--rm");
    }
    command
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
    // A linked worktree's `.git` file points at the shared cache repo
    // (outside the workspace mount). Bind-mount that repo at the identical
    // path so in-sandbox git resolves — and commits flow back to the
    // shared object store.
    if let Some(repo) = linked_repo_dir(&workspace) {
        let repo = repo.display();
        command.arg("-v").arg(format!("{repo}:{repo}"));
    }
    for mount in spec.extra_mounts {
        command.arg("-v").arg(mount);
    }
    command.arg("-w").arg(cwd_mount_path(cwd_rel));
    for (k, v) in SANDBOX_ENV {
        command.arg("-e").arg(format!("{k}={v}"));
    }
    for (k, v) in extra_env {
        command.arg("-e").arg(format!("{k}={v}"));
    }
    command.arg(spec.image);
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
        force_remove(&name);
        // `podman run` passes the container's exit code through: a non-zero
        // status is usually the command itself failing (a probe, `grep` with
        // no match, …), not a podman/image problem. Report stdout too —
        // probes often print to stdout and exit non-zero with empty stderr.
        let mut msg = String::from_utf8_lossy(&err_buf).into_owned();
        let out_text = String::from_utf8_lossy(&out_buf).into_owned();
        if !out_text.trim().is_empty() {
            msg.push_str(&out_text);
        }
        truncate_bytes(&mut msg, ERR_REPORT_CAP);
        if msg.trim().is_empty() {
            // No output at all — only then is a missing podman/image likely.
            msg = format!(
                "podman exited with {status} (missing `{PODMAN_BIN}` or image `{}`?)",
                spec.image
            );
        }
        msg = format!(
            "<error> {}\ncommand exited with {}",
            msg.trim_end(),
            status.code().unwrap_or(-1)
        );
        return Err(Error::other(msg));
    }
    if let Some(tag) = spec.cache_tag
        && let Err(e) = super::image::commit_and_remove(&name, tag)
    {
        eprintln!("sandbox: image cache commit failed: {e}");
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

/// For a linked worktree workspace: the shared repo dir its `.git` file
/// points into (`<repo>/worktrees/<name>` → `<repo>`). None when the
/// workspace is not a linked worktree (`.git` is a real dir or missing).
fn linked_repo_dir(workspace: &Path) -> Option<PathBuf> {
    let link = fs::read_to_string(workspace.join(".git")).ok()?;
    let gitdir = link.trim().strip_prefix("gitdir:")?.trim();
    // <repo>/worktrees/<admin-name> → <repo>
    Path::new(gitdir).ancestors().nth(2).map(Path::to_path_buf)
}

fn force_remove(name: &str) {
    let _ = Command::new(PODMAN_BIN)
        .args(["rm", "-f", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
