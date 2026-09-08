//! Rootless Linux sandbox: user/PID/mount/net/IPC/UTS namespaces, private
//! mount layout with `pivot_root`, capability drop, seccomp deny-list,
//! rlimits, and a writable `/workspace`.
//!
//! # Security model
//!
//! ## Architecture
//! ```text
//! backend (unprivileged uid, never root)
//!   └─ fork
//!        └─ unshare(CLONE_NEWUSER)          ← no root required
//!             write uid_map: 0 <backend-uid> 1
//!             write gid_map: 0 <backend-gid> 1
//!        └─ unshare(CLONE_NEWNS|NEWPID|NEWNET|NEWIPC|NEWUTS)
//!        └─ mounts (all private to this namespace)
//!             /            MS_REC|MS_PRIVATE
//!             <rootfs>/    bind of the staged rootfs (pivot target)
//!             /bin /usr /lib /lib64   read-only binds of host dirs
//!             /tmp         fresh tmpfs (1777)
//!             /dev         fresh tmpfs + bound null/zero/random/urandom
//!             /proc        fresh procfs (only sandbox processes visible)
//!             /workspace   read-write bind of the unique workspace dir
//!        └─ pivot_root(new_root, old_root); umount old root
//!        └─ rlimits: NPROC, AS, CPU, FSIZE, NOFILE, STACK
//!        └─ seccomp deny-list (see `seccomp` module docs)
//!        └─ exec /bin/bash -c "<cmd>"   (bash is PID 1 of the PID namespace)
//! ```
//!
//! ## What this protects against
//! - Host filesystem access: the sandbox root contains only read-only system
//!   dirs and the workspace; there is **no** path to `/home`, `/root`,
//!   `/etc` (deliberately not mounted) or other host data. `cat /etc/passwd`,
//!   `cat ~/.ssh/id_rsa` etc. fail at the kernel level.
//! - Lexical escapes (`..`, absolute paths, `cd /`): the boundary is the
//!   mount namespace + pivot_root, not path validation.
//! - Host network: dedicated network namespace whose loopback is down.
//! - Host process visibility: dedicated PID namespace; `ps` shows only
//!   sandbox processes. The shell is PID 1 — when it exits or is SIGKILLed
//!   the namespace is destroyed, terminating all descendants. No orphans.
//! - Fork bombs / exhaustion: RLIMIT_NPROC/AS/CPU/FSIZE + wall-clock timeout
//!   + output caps.
//! - Escalation primitives: seccomp denies mount/umount/pivot_root/setns/
//!   unshare/ptrace/kexec/module-load/bpf/keyring/reboot/chroot/… and the
//!   kernel's capability rules already confine user-namespace "root" to
//!   namespace-owned objects.
//! - Secret leakage: the environment is a fixed allow-list; host env never
//!   reaches the shell.
//!
//! ## What this does NOT guarantee (honest limitations)
//! - Linux kernel vulnerabilities (userns/namespace/seccomp CVEs).
//! - Privileged host processes: root on the host can inspect/kill everything.
//! - Vulnerabilities in the mounted read-only trees (`/bin`, `/usr`, `/lib`,
//!   `/lib64`) — supply-chain attacks remain in scope.
//! - `NetworkPolicy::Enabled` only keeps the isolated netns (loopback only);
//!   Internet access needs host-side veth/SLIRP plumbing (privileged helper)
//!   which is out of scope. There is no accidental host-network inheritance
//!   in either mode.
//! - `/etc` is intentionally not mounted (no host account/config data); as a
//!   consequence glibc NSS name resolution does not work inside.
//! - Availability: a sandbox can still fill its own 64 MiB tmpfs or burn its
//!   CPU allowance.
//!
//! ## Kernel prerequisites
//! - Unprivileged user namespaces enabled
//!   (`sysctl kernel.unprivileged_userns_clone=1` on Debian/Ubuntu kernels).
//! - `unshare(2)` permitted (no seccomp/container policy blocking it for the
//!   backend). If namespace creation fails, this module returns an error —
//!   it NEVER falls back to running the command unsandboxed.

use std::ffi::CString;
use std::fs;
use std::io::{self, Error, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::sandbox_abstract_layer::{Guarantee, SandboxLayer};

// Canonical shared types live in the abstract layer; re-exported here to
// keep this module's public API identical to the other platforms.
pub use crate::sandbox_abstract_layer::{HistoryEntry, Role, SandboxState};

pub const SHELL: &str = "/bin/bash";
pub const RUN_FLAG: &str = "-c";
pub const MAX_OUTPUT_BYTES: usize = 1 << 20;
pub const MAX_HISTORY: usize = 128;
pub const SANDBOX_PREFIX: &str = "susutaku-agent-sandbox-";
pub const STATE_FILE: &str = "agent-sandbox-state.json";

const WORKSPACE_MOUNT: &str = "workspace"; // inside the sandbox root staging dir
const ROOTFS_SUBDIR: &str = "rootfs";
const SANDBOX_SUBDIR: &str = "sandbox";
const OLD_ROOT: &str = "old_root";
const METADATA_FILE: &str = "sandbox-metadata.json";
const URANDOM: &str = "/dev/urandom";
const ID_BYTES: usize = 16;

/// Host dirs bind-mounted read-only into the sandbox root.
const RO_SHARE_DIRS: [&str; 4] = ["/bin", "/usr", "/lib", "/lib64"];
const DEV_BIND_NODES: [&str; 4] = ["null", "zero", "random", "urandom"];
const DEV_TMPFS_OPTS: &str = "mode=755,size=1m";
const TMP_TMPFS_OPTS: &str = "mode=1777,size=64m";

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_MAX_PROCESSES: u64 = 64;
const DEFAULT_NOFILE: u64 = 64;
const DEFAULT_STACK_BYTES: u64 = 8 * 1024 * 1024;
const OUTPUT_HEADROOM: usize = 2;
const ERR_REPORT_CAP: usize = 4096;
const POLL_INTERVAL_MS: u64 = 20;
const CHILD_EXIT_SETUP_FAILED: i32 = 126;

/// Variables allowed into the sandboxed shell — nothing else. Host secrets
/// (`AWS_*`, `DATABASE_URL`, `*_TOKEN`, `SSH_AUTH_SOCK`, real `HOME`, …) can
/// never leak through the environment.
const SANDBOX_ENV: [(&str, &str); 5] = [
    ("PATH", "/usr/bin:/bin:/usr/sbin:/sbin"),
    ("HOME", "/workspace"),
    ("USER", "sandbox"),
    ("TMPDIR", "/tmp"),
    ("LANG", "C.UTF-8"),
];

#[derive(Debug, Clone)]
pub enum NetworkPolicy {
    /// Private netns, loopback down. No host network.
    Disabled,
    /// Private netns kept; still loopback-only (see module docs).
    Enabled,
}

/// `Default`-friendly, `Copy` mirror of [`NetworkPolicy`] for config structs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetworkPolicyChoice {
    #[default]
    Disabled,
    Enabled,
}

impl From<NetworkPolicyChoice> for NetworkPolicy {
    fn from(c: NetworkPolicyChoice) -> Self {
        match c {
            NetworkPolicyChoice::Disabled => NetworkPolicy::Disabled,
            NetworkPolicyChoice::Enabled => NetworkPolicy::Enabled,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SandboxLimits {
    pub timeout: Duration,
    pub max_output_bytes: usize,
    pub max_processes: u64,
    pub max_memory_bytes: Option<u64>,
    pub max_cpu_seconds: Option<u64>,
    pub max_file_size_bytes: Option<u64>,
}

impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            max_output_bytes: MAX_OUTPUT_BYTES,
            max_processes: DEFAULT_MAX_PROCESSES,
            max_memory_bytes: None,
            max_cpu_seconds: None,
            max_file_size_bytes: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub limits: SandboxLimits,
    pub network: NetworkPolicyChoice,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            limits: SandboxLimits::default(),
            network: NetworkPolicyChoice::Disabled,
        }
    }
}

/// Ownership metadata persisted inside every sandbox dir. A stale dir is only
/// deleted when this file exists and its owner is provably dead.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SandboxMetadata {
    sandbox_id: String,
    owner_pid: u32,
    created_at_unix: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lifecycle {
    Idle,
    Running,
    Purging,
    Purged,
}

pub struct Sandbox {
    sandbox_id: String,
    root: PathBuf,
    state_file: PathBuf,
    state: RwLock<SandboxState>,
    lifecycle: RwLock<Lifecycle>,
    /// `run` holds a read guard; `purge` takes the write guard, so a
    /// workspace can never be deleted mid-command.
    run_gate: RwLock<()>,
}

/// Free-function run inside a one-off sandbox (API compat).
pub fn run(cmd: &str) -> Result<String, Error> {
    let sb = Sandbox::new()?;
    let out = sb.run(cmd)?;
    sb.purge();
    Ok(out)
}

impl Sandbox {
    pub fn new() -> Result<Self, Error> {
        purge_stale_dirs();
        purge_stale_state();
        let sandbox_id = random_hex_id()?;
        let root = sandbox_root(&sandbox_id);
        create_workspace_layout(&root)?;
        write_metadata(&root, &sandbox_id)?;
        let state_file = instance_state_dir()?.join(format!("{sandbox_id}.json"));
        Ok(Self {
            sandbox_id,
            root,
            state_file,
            state: RwLock::new(SandboxState::default()),
            lifecycle: RwLock::new(Lifecycle::Idle),
            run_gate: RwLock::new(()),
        })
    }

    /// Sandbox rooted at an explicit work tree (one per agent). The usual
    /// rootfs/workspace layout is created inside the given tree.
    pub fn new_in(work_tree: &Path) -> Result<Self, Error> {
        let sandbox_id = random_hex_id()?;
        fs::create_dir_all(work_tree)?;
        let root = work_tree.join(SANDBOX_SUBDIR);
        create_workspace_layout(&root)?;
        write_metadata(&root, &sandbox_id)?;
        let state_file = instance_state_dir()?.join(format!("{sandbox_id}.json"));
        Ok(Self {
            sandbox_id,
            root,
            state_file,
            state: RwLock::new(SandboxState::default()),
            lifecycle: RwLock::new(Lifecycle::Idle),
            run_gate: RwLock::new(()),
        })
    }

    /// Reload the most recently saved state belonging to *this backend
    /// instance* and recreate its workspace.
    pub fn restore() -> Result<Self, Error> {
        let state_dir = instance_state_dir()?;
        let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
        for entry in fs::read_dir(&state_dir)?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            // Validate it parses before considering it.
            let raw = fs::read_to_string(&path)?;
            if serde_json::from_str::<SandboxState>(&raw).is_err() {
                continue;
            }
            let mtime = entry.metadata().and_then(|m| m.modified())?;
            if best.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true) {
                best = Some((mtime, path));
            }
        }
        let Some((_, path)) = best else {
            return Err(Error::other("no saved sandbox state for this instance"));
        };
        let raw = fs::read_to_string(&path)?;
        let state: SandboxState =
            serde_json::from_str(&raw).map_err(|e| Error::other(e.to_string()))?;
        let sandbox_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| Error::other("malformed state file name"))?
            .to_owned();
        let root = sandbox_root(&sandbox_id);
        create_workspace_layout(&root)?;
        Ok(Self {
            sandbox_id,
            root,
            state_file: path,
            state: RwLock::new(state),
            lifecycle: RwLock::new(Lifecycle::Idle),
            run_gate: RwLock::new(()),
        })
    }

    /// Remove leftovers of dead backend instances (call at startup).
    pub fn purge_stale() {
        purge_stale_dirs();
        purge_stale_state();
    }

    /// Writable workspace (host-side path; mounted at `/workspace` inside).
    pub fn root(&self) -> PathBuf {
        self.root.join(WORKSPACE_MOUNT)
    }

    pub fn run(&self, cmd: &str) -> Result<String, Error> {
        self.run_with(
            cmd,
            &SandboxLimits::default(),
            NetworkPolicyChoice::Disabled,
        )
    }

    pub fn run_with(
        &self,
        cmd: &str,
        limits: &SandboxLimits,
        network: NetworkPolicyChoice,
    ) -> Result<String, Error> {
        {
            let lc = self
                .lifecycle
                .read()
                .map_err(|e| Error::other(e.to_string()))?;
            if matches!(*lc, Lifecycle::Purging | Lifecycle::Purged) {
                return Err(Error::other("sandbox already purged"));
            }
        }
        let _gate = self
            .run_gate
            .read()
            .map_err(|e| Error::other(e.to_string()))?;
        if let Ok(mut lc) = self.lifecycle.write() {
            *lc = Lifecycle::Running;
        }
        let result = self.run_inner(cmd, limits, network).and_then(|text| {
            self.record(HistoryEntry {
                role: Role::Tool,
                content: format!("$ {cmd}\n{text}"),
            })
            .map(|_| text)
        });
        if let Ok(mut lc) = self.lifecycle.write() {
            if *lc == Lifecycle::Running {
                *lc = Lifecycle::Idle;
            }
        }
        result
    }

    fn run_inner(
        &self,
        cmd: &str,
        limits: &SandboxLimits,
        network: NetworkPolicyChoice,
    ) -> Result<String, Error> {
        let cwd_rel = self
            .state
            .read()
            .map_err(|e| Error::other(e.to_string()))?
            .cwd
            .clone();
        let cwd_in_sandbox = cwd_mount_path(&self.root, &cwd_rel);

        let (pid, out_rd, err_rd) =
            spawn_namespaced_child(cmd, &self.root, &cwd_in_sandbox, limits, network)?;

        let deadline = Instant::now() + limits.timeout;
        let limits_cloned = limits.clone();
        let max_out = limits.max_output_bytes;
        let reader = std::thread::spawn(move || {
            let mut out = Vec::new();
            let mut err = Vec::new();
            let mut o = out_rd;
            let mut e = err_rd;
            drain(&mut o, max_out, &mut out);
            drain(&mut e, max_out, &mut err);
            let _still_owned = limits_cloned;
            (out, err)
        });

        let mut status = 0;
        let exited = loop {
            let got = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
            if got == pid {
                break true;
            }
            if got < 0 {
                return Err(Error::last_os_error());
            }
            if Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        };
        if !exited {
            // PID 1 of the PID namespace: SIGKILL destroys the namespace and
            // every descendant with it.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
                libc::waitpid(pid, &mut status, 0);
            }
            let _ = reader.join();
            return Err(Error::other(format!(
                "sandbox command timed out after {}s",
                limits.timeout.as_secs()
            )));
        }
        let (out, err) = reader.join().unwrap_or_default();
        let success = libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0;
        if !success {
            let mut msg = String::from_utf8_lossy(&err).into_owned();
            truncate_bytes(&mut msg, ERR_REPORT_CAP);
            return Err(Error::other(msg));
        }
        let mut text = String::from_utf8_lossy(&out).into_owned();
        truncate_bytes(&mut text, limits.max_output_bytes);
        Ok(text)
    }

    pub fn push_context(&self, role: Role, content: impl Into<String>) {
        let entry = HistoryEntry {
            role,
            content: content.into(),
        };
        if let Err(e) = self.record(entry) {
            // Persistence failure of pushed context is reported, not dropped.
            eprintln!("sandbox: failed to persist context: {e}");
        }
    }

    pub fn transcript(&self) -> Vec<HistoryEntry> {
        self.state
            .read()
            .map(|s| s.history.clone())
            .unwrap_or_default()
    }

    pub fn set_cwd(&self, rel: &str) -> Result<(), Error> {
        let workspace = self.root();
        let p = resolve_cwd(&self.root, rel);
        if !p.starts_with(&workspace) {
            return Err(Error::other("cwd escapes the sandbox workspace"));
        }
        let rel_clean = p
            .strip_prefix(&workspace)
            .map(|r| r.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Ok(mut s) = self.state.write() {
            s.cwd = if rel_clean.is_empty() {
                ".".into()
            } else {
                rel_clean
            };
        }
        self.save()
    }

    /// Full teardown: blocks until in-flight runs finish, then deletes the
    /// workspace and state file. (The kernel namespace dies with its last
    /// process; no host-side handles remain.)
    pub fn purge(&self) {
        let Ok(_gate) = self.run_gate.write() else {
            return; // poisoned lock: another purge already tore things down
        };
        if let Ok(mut lc) = self.lifecycle.write() {
            *lc = Lifecycle::Purging;
        }
        let _ = fs::remove_dir_all(&self.root);
        let _ = fs::remove_file(&self.state_file);
        if let Ok(mut lc) = self.lifecycle.write() {
            *lc = Lifecycle::Purged;
        }
    }

    fn record(&self, entry: HistoryEntry) -> Result<(), Error> {
        {
            let mut s = self
                .state
                .write()
                .map_err(|e| Error::other(e.to_string()))?;
            s.history.push(entry);
            let excess = s.history.len().saturating_sub(MAX_HISTORY);
            s.history.drain(..excess);
        }
        // Transcript persistence is security-relevant (must not silently
        // vanish), hence a real error rather than `let _ =`.
        self.save()
    }

    fn save(&self) -> Result<(), Error> {
        let snapshot = self
            .state
            .read()
            .map_err(|e| Error::other(e.to_string()))?
            .clone();
        let raw =
            serde_json::to_string_pretty(&snapshot).map_err(|e| Error::other(e.to_string()))?;
        fs::write(&self.state_file, raw)
    }
}

impl SandboxLayer for Sandbox {
    fn guarantee(&self) -> Guarantee {
        Guarantee::Kernel
    }
    fn root(&self) -> PathBuf {
        Sandbox::root(self)
    }
    fn run(&self, cmd: &str) -> Result<String, Error> {
        Sandbox::run(self, cmd)
    }
    fn push_context(&self, role: Role, content: impl Into<String>) {
        Sandbox::push_context(self, role, content)
    }
    fn transcript(&self) -> Vec<HistoryEntry> {
        Sandbox::transcript(self)
    }
    fn set_cwd(&self, rel: &str) -> Result<(), Error> {
        Sandbox::set_cwd(self, rel)
    }
    fn purge(&self) {
        Sandbox::purge(self)
    }
    fn purge_stale() {
        Sandbox::purge_stale()
    }
    fn id(&self) -> &str {
        &self.sandbox_id
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if self.run_gate.try_write().is_ok() {
            let _ = fs::remove_dir_all(&self.root);
        }
        // If a run holds the gate we leave the dir; the run's own teardown
        // and the PID namespace kill the process side regardless.
    }
}

pub fn state_path() -> PathBuf {
    instance_state_dir().unwrap_or_else(|_| std::env::temp_dir().join("susutaku-agent-state"))
}

// ---------------------------------------------------------------------------
// Child process: namespaces + mounts + pivot + limits + seccomp + exec
// ---------------------------------------------------------------------------

/// Sentinel on the CLOEXEC setup pipe: a failed setup writes this before
/// _exit; successful exec simply closes the pipe (parent reads EOF).
const EXEC_SETUP_FAILED: u8 = b'x';

fn spawn_namespaced_child(
    cmd: &str,
    root: &Path,
    cwd_in_sandbox: &str,
    limits: &SandboxLimits,
    network: NetworkPolicyChoice,
) -> Result<(libc::pid_t, fs::File, fs::File), Error> {
    use std::os::unix::io::FromRawFd;

    let (out_rd, out_wr) = inheritable_pipe()?;
    let (err_rd, err_wr) = inheritable_pipe()?;
    // CLOEXEC pair: closes on successful exec → parent sees EOF = "running".
    let (setup_rd, setup_wr) = cloexec_pipe()?;

    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };

    let shell_c = CString::new(SHELL).map_err(|e| Error::other(e.to_string()))?;
    let run_flag_c = CString::new(RUN_FLAG).map_err(|e| Error::other(e.to_string()))?;
    let cmd_c = CString::new(cmd).map_err(|e| Error::other(e.to_string()))?;
    let cwd_c = CString::new(cwd_in_sandbox).map_err(|e| Error::other(e.to_string()))?;
    let root_c = path_to_cstring(root)?;
    let envp = exec_env_block()?;

    // SAFETY: fork() in a multithreaded backend restricts the child to
    // async-signal-safe functions. The child deliberately uses only raw libc
    // syscalls and CStrings allocated *before* the fork, and never touches
    // the parent's allocator state beyond that.
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(Error::last_os_error());
    }
    if pid == 0 {
        child_body(
            &shell_c,
            &run_flag_c,
            &cmd_c,
            &cwd_c,
            &root_c,
            &envp,
            limits,
            network,
            uid,
            gid,
            setup_wr,
            out_wr,
            err_wr,
        );
        // unreachable
    }

    // Parent: close the child-side ends.
    unsafe {
        libc::close(setup_wr);
        libc::close(out_wr);
        libc::close(err_wr);
    }

    // Detect exec success: the CLOEXEC setup pipe closes on exec; a failed
    // setup writes an explicit sentinel before _exit.
    let mut sentinel = [0u8; 1];
    let read = unsafe {
        libc::read(
            setup_rd,
            sentinel.as_mut_ptr().cast(),
            sentinel.len() as libc::size_t,
        )
    };
    unsafe {
        libc::close(setup_rd);
    }
    if read < 0 {
        return Err(Error::last_os_error());
    }
    if read > 0 && sentinel[0] == EXEC_SETUP_FAILED {
        let mut status = 0;
        unsafe {
            libc::kill(pid, libc::SIGKILL);
            libc::waitpid(pid, &mut status, 0);
        }
        return Err(Error::other(
            "sandbox setup failed inside the child (namespace/mount/seccomp)",
        ));
    }
    // read == 0: the CLOEXEC setup pipe closed via successful exec.

    // SAFETY: fd ownership moves into these Files; child ends were closed
    // in the parent above, so no duplicate handle remains.
    let out = unsafe { fs::File::from_raw_fd(out_rd) };
    let err = unsafe { fs::File::from_raw_fd(err_rd) };
    Ok((pid, out, err))
}

/// Everything the forked child does. Never returns.
#[allow(clippy::too_many_arguments)]
fn child_body(
    shell_c: &CString,
    run_flag_c: &CString,
    cmd_c: &CString,
    cwd_c: &CString,
    root_c: &CString,
    envp: &[CString],
    limits: &SandboxLimits,
    network: NetworkPolicyChoice,
    uid: u32,
    gid: u32,
    setup_wr: i32,
    out_wr: i32,
    err_wr: i32,
) -> ! {
    let _ = network; // netns isolation is unconditional; see module docs
    // Order matters: user ns first (needs no privileges), then the mapped
    // id enables the remaining unshares; mounts before seccomp (the filter
    // must not break setup); seccomp before exec (untrusted code last).
    unsafe {
        libc::setsid();

        // 1. User namespace: map the backend's unprivileged host identity
        //    to uid/gid 0 *inside* the namespace only.
        if libc::unshare(libc::CLONE_NEWUSER) != 0 {
            child_fail(setup_wr);
        }
        if !write_id_map("/proc/self/setgroups", "deny")
            || !write_id_map("/proc/self/uid_map", &format!("0 {uid} 1\n"))
            || !write_id_map("/proc/self/gid_map", &format!("0 {gid} 1\n"))
        {
            child_fail(setup_wr);
        }

        // 2. Isolation namespaces. `CLONE_NEWNET` gives an empty netns
        //    (loopback only, down) in BOTH network policies — see docs.
        let ns_flags = libc::CLONE_NEWNS
            | libc::CLONE_NEWPID
            | libc::CLONE_NEWNET
            | libc::CLONE_NEWIPC
            | libc::CLONE_NEWUTS;
        if libc::unshare(ns_flags) != 0 {
            child_fail(setup_wr);
        }

        // 3. Private mount tree + pivot_root into the staged rootfs.
        if !mount_setup(root_c) {
            child_fail(setup_wr);
        }

        // 4. Resource limits (NPROC / AS / CPU / FSIZE / NOFILE / STACK).
        if !apply_rlimits(limits) {
            child_fail(setup_wr);
        }

        // 5. seccomp deny-list — last security step before exec.
        if seccomp::apply().is_err() {
            child_fail(setup_wr);
        }

        // 6. stdio + exec. The shell becomes PID 1 of the PID namespace:
        //    its exit (or SIGKILL) destroys the namespace and every
        //    descendant — no orphaned processes.
        libc::dup2(out_wr, libc::STDOUT_FILENO);
        libc::dup2(err_wr, libc::STDERR_FILENO);
        libc::chdir(cwd_c.as_ptr());
        let argv = [
            shell_c.as_ptr(),
            run_flag_c.as_ptr(),
            cmd_c.as_ptr(),
            std::ptr::null(),
        ];
        let mut envp_ptrs: Vec<*const libc::c_char> = envp.iter().map(|e| e.as_ptr()).collect();
        envp_ptrs.push(std::ptr::null());
        libc::execve(shell_c.as_ptr(), argv.as_ptr(), envp_ptrs.as_ptr());
        child_fail(setup_wr);
    }
}

fn child_fail(setup_wr: i32) -> ! {
    unsafe {
        let b: u8 = EXEC_SETUP_FAILED;
        libc::write(setup_wr, (&b as *const u8).cast(), 1);
        libc::_exit(CHILD_EXIT_SETUP_FAILED);
    }
}

fn write_id_map(path: &str, content: &str) -> bool {
    let c = match CString::new(path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let fd = unsafe { libc::open(c.as_ptr(), libc::O_WRONLY) };
    if fd < 0 {
        return false;
    }
    let bytes = content.as_bytes();
    let ok = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) } as usize == bytes.len();
    unsafe {
        libc::close(fd);
    }
    ok
}

/// Mount-tree staging + pivot. Runs inside the forked child with all
/// namespaces active. Returns false on any failure (fail closed).
fn mount_setup(root_c: &CString) -> bool {
    unsafe {
        // 1. Stop propagation of our mounts to the host.
        if libc::mount(
            std::ptr::null(),
            c"/".as_ptr(),
            std::ptr::null(),
            libc::MS_REC | libc::MS_PRIVATE,
            std::ptr::null(),
        ) != 0
        {
            return false;
        }
        let rootfs = join_cstring(root_c, ROOTFS_SUBDIR);

        // 2. Make the staging root a mount point (pivot_root requirement).
        if libc::mount(
            rootfs.as_ptr(),
            rootfs.as_ptr(),
            std::ptr::null(),
            libc::MS_BIND | libc::MS_REC,
            std::ptr::null(),
        ) != 0
        {
            return false;
        }

        // 3. Read-only host system trees.
        for dir in RO_SHARE_DIRS {
            let host = match CString::new(dir) {
                Ok(c) => c,
                Err(_) => return false,
            };
            let target = join_cstring(root_c, dir.trim_start_matches('/'));
            if libc::mkdir(target.as_ptr(), 0o755) != 0 && !is_eexist() {
                return false;
            }
            if libc::mount(
                host.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                libc::MS_BIND | libc::MS_REC,
                std::ptr::null(),
            ) != 0
            {
                return false;
            }
            // RDONLY needs a remount of the bind mount to take effect.
            if libc::mount(
                std::ptr::null(),
                target.as_ptr(),
                std::ptr::null(),
                libc::MS_BIND | libc::MS_RDONLY | libc::MS_REMOUNT | libc::MS_REC,
                std::ptr::null(),
            ) != 0
            {
                return false;
            }
        }

        // 4. Fresh tmpfs scratch + dev; proc for the new PID namespace.
        if !mount_tmpfs(&join_cstring(root_c, "tmp"), TMP_TMPFS_OPTS) {
            return false;
        }
        if !mount_tmpfs(&join_cstring(root_c, "dev"), DEV_TMPFS_OPTS) {
            return false;
        }
        let proc_target = join_cstring(root_c, "proc");
        if libc::mkdir(proc_target.as_ptr(), 0o755) != 0 && !is_eexist() {
            return false;
        }
        if libc::mount(
            c"proc".as_ptr(),
            proc_target.as_ptr(),
            c"proc".as_ptr(),
            libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC,
            std::ptr::null(),
        ) != 0
        {
            return false;
        }

        // 5. Minimal device nodes (bind host char devices; no data access).
        for node in DEV_BIND_NODES {
            let host = match CString::new(format!("/dev/{node}")) {
                Ok(c) => c,
                Err(_) => return false,
            };
            let target = join_cstring(root_c, &format!("dev/{node}"));
            if libc::close(libc::open(
                target.as_ptr(),
                libc::O_CREAT | libc::O_WRONLY,
                0o666,
            )) < 0
                && !is_eexist()
            {
                return false;
            }
            if libc::mount(
                host.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                libc::MS_BIND,
                std::ptr::null(),
            ) != 0
            {
                return false;
            }
        }
        // Standard /dev symlinks.
        for (link, target) in [
            ("fd", "/proc/self/fd"),
            ("stdin", "/proc/self/fd/0"),
            ("stdout", "/proc/self/fd/1"),
            ("stderr", "/proc/self/fd/2"),
        ] {
            let lp = join_cstring(root_c, &format!("dev/{link}"));
            let tp = match CString::new(target) {
                Ok(c) => c,
                Err(_) => return false,
            };
            libc::symlink(tp.as_ptr(), lp.as_ptr());
        }

        // 6. The ONLY writable host bind: this sandbox's workspace.
        let ws_path = join_cstring(root_c, WORKSPACE_MOUNT);
        if libc::mount(
            ws_path.as_ptr(),
            ws_path.as_ptr(),
            std::ptr::null(),
            libc::MS_BIND | libc::MS_REC,
            std::ptr::null(),
        ) != 0
        {
            return false;
        }

        // 7. pivot_root into the staged tree; detach the host tree.
        let old = join_cstring(root_c, OLD_ROOT);
        if libc::mkdir(old.as_ptr(), 0o700) != 0 && !is_eexist() {
            return false;
        }
        if pivot_root(rootfs.as_ptr(), old.as_ptr()) != 0 {
            return false;
        }
        if libc::chdir(c"/".as_ptr()) != 0 {
            return false;
        }
        let old_in_new = match CString::new(format!("/{OLD_ROOT}")) {
            Ok(c) => c,
            Err(_) => return false,
        };
        if libc::umount2(old_in_new.as_ptr(), libc::MNT_DETACH) != 0 {
            return false;
        }
        true
    }
}

// libc crate does not export `pivot_root`; declare it directly.
extern "C" {
    fn pivot_root(new_root: *const libc::c_char, put_old: *const libc::c_char) -> libc::c_int;
}

fn is_eexist() -> bool {
    io::Error::last_os_error().kind() == io::ErrorKind::AlreadyExists
}

unsafe fn mount_tmpfs(target: &CString, opts: &str) -> bool {
    let opts_c = match CString::new(opts) {
        Ok(c) => c,
        Err(_) => return false,
    };
    libc::mount(
        c"tmpfs".as_ptr(),
        target.as_ptr(),
        c"tmpfs".as_ptr(),
        0,
        opts_c.as_ptr().cast(),
    ) == 0
}

fn apply_rlimits(limits: &SandboxLimits) -> bool {
    unsafe fn set(res: libc::__rlimit_resource_t, val: u64) -> bool {
        let lim = libc::rlimit {
            rlim_cur: val as libc::rlim_t,
            rlim_max: val as libc::rlim_t,
        };
        libc::setrlimit(res, &lim) == 0
    }
    unsafe {
        if !set(libc::RLIMIT_NPROC, limits.max_processes.max(1)) {
            return false;
        }
        if !set(libc::RLIMIT_NOFILE, DEFAULT_NOFILE) {
            return false;
        }
        if !set(libc::RLIMIT_STACK, DEFAULT_STACK_BYTES) {
            return false;
        }
        if let Some(mem) = limits.max_memory_bytes {
            if !set(libc::RLIMIT_AS, mem) {
                return false;
            }
        }
        if let Some(cpu) = limits.max_cpu_seconds {
            if !set(libc::RLIMIT_CPU, cpu) {
                return false;
            }
        }
        if let Some(fsz) = limits.max_file_size_bytes {
            if !set(libc::RLIMIT_FSIZE, fsz) {
                return false;
            }
        }
    }
    true
}

fn exec_env_block() -> Result<Vec<CString>, Error> {
    SANDBOX_ENV
        .iter()
        .map(|(k, v)| CString::new(format!("{k}={v}")).map_err(|e| Error::other(e.to_string())))
        .collect()
}

// ---------------------------------------------------------------------------
// seccomp (seccompiler, pure-Rust BPF)
// ---------------------------------------------------------------------------

/// Conservative deny-list of kernel operations with no legitimate use in a
/// command-execution sandbox.
///
/// **Limitations (explicit):** argument-insensitive rules (e.g. `clone` is
/// allowed with any flags — but `unshare`/`setns` are denied, and the user
/// namespace already confines fresh namespaces); syscall tables for x86_64
/// and aarch64 only; on any other architecture the child fails closed
/// rather than running unfiltered.
mod seccomp {
    use std::collections::BTreeMap;

    use seccompiler::{SeccompAction, SeccompFilter, SeccompRule, TargetArch};

    const DENIED: &[i64] = &[
        libc::SYS_mount,
        libc::SYS_umount2,
        libc::SYS_pivot_root,
        libc::SYS_setns,
        libc::SYS_unshare,
        libc::SYS_ptrace,
        libc::SYS_reboot,
        libc::SYS_kexec_load,
        libc::SYS_kexec_file_load,
        libc::SYS_init_module,
        libc::SYS_finit_module,
        libc::SYS_delete_module,
        libc::SYS_swapon,
        libc::SYS_swapoff,
        libc::SYS_iopl,
        libc::SYS_ioperm,
        libc::SYS_open_by_handle_at,
        libc::SYS_name_to_handle_at,
        libc::SYS_bpf,
        libc::SYS_keyctl,
        libc::SYS_add_key,
        libc::SYS_request_key,
        libc::SYS_acct,
        libc::SYS_perf_event_open,
        libc::SYS_userfaultfd,
        libc::SYS_chroot,
    ];

    pub fn apply() -> Result<(), String> {
        let arch = if cfg!(target_arch = "x86_64") {
            TargetArch::x86_64
        } else if cfg!(target_arch = "aarch64") {
            TargetArch::aarch64
        } else {
            return Err("no seccomp table for this architecture; refusing to run".into());
        };
        let rules: BTreeMap<i64, Vec<SeccompRule>> = DENIED
            .iter()
            .map(|&nr| {
                let rule = SeccompRule::new(vec![]).map_err(|e| e.to_string())?;
                Ok((nr, vec![rule]))
            })
            .collect::<Result<_, String>>()?;
        // Matched syscall => EPERM; everything else => allowed.
        let filter = SeccompFilter::new(
            rules,
            SeccompAction::Allow,
            SeccompAction::Errno(libc::EPERM as u32),
            arch,
        )
        .map_err(|e| e.to_string())?;
        let bpf: seccompiler::BpfProgram =
            TryInto::<seccompiler::BpfProgram>::try_into(filter).map_err(|e| e.to_string())?;
        seccompiler::apply_filter(&bpf).map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Stale-sandbox management (ownership metadata + PID-reuse guard)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct SandboxDir {
    pub pid: u32,
    pub alive: bool,
    pub path: PathBuf,
    pub sandbox_id: Option<String>,
}

pub fn list_dirs() -> Vec<SandboxDir> {
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return Vec::new();
    };
    let mut dirs: Vec<SandboxDir> = entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(SANDBOX_PREFIX))
        .map(|e| {
            let meta = read_metadata(&e.path());
            let alive = meta
                .as_ref()
                .map(|m| pid_alive_and_owned(m.owner_pid, &e.path()))
                .unwrap_or(false);
            SandboxDir {
                pid: meta.as_ref().map(|m| m.owner_pid).unwrap_or(0),
                alive,
                path: e.path(),
                sandbox_id: meta.map(|m| m.sandbox_id),
            }
        })
        .collect();
    dirs.sort_by(|a, b| a.path.cmp(&b.path));
    dirs
}

fn read_metadata(path: &Path) -> Option<SandboxMetadata> {
    let raw = fs::read_to_string(path.join(METADATA_FILE)).ok()?;
    serde_json::from_str(&raw).ok()
}

/// PID-reuse guard: the dir must have been created *after* its owner process
/// started. Inconclusive evidence ⇒ treated as owned (never delete on doubt).
fn pid_alive_and_owned(pid: u32, dir: &Path) -> bool {
    if !pid_alive(pid) {
        return false;
    }
    match (process_start_time(pid), dir_create_time(dir)) {
        (Some(p), Some(d)) => d >= p,
        _ => true,
    }
}

fn purge_stale_dirs() {
    let own_pid = std::process::id();
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(SANDBOX_PREFIX)
        {
            continue;
        }
        let Some(meta) = read_metadata(&entry.path()) else {
            // No ownership metadata ⇒ no live owner can be established.
            let _ = fs::remove_dir_all(entry.path());
            continue;
        };
        if meta.owner_pid == own_pid {
            continue;
        }
        if !pid_alive_and_owned(meta.owner_pid, &entry.path()) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// Delete the sandbox owned by `pid`. Refuses the caller's own sandbox and
/// any dir whose owner cannot be established as dead.
pub fn purge_dir(pid: u32) -> Result<bool, String> {
    if pid == std::process::id() {
        return Err("cannot purge the running backend's own sandbox".into());
    }
    let Some(entry) = list_dirs().into_iter().find(|d| d.pid == pid) else {
        return Ok(false);
    };
    if pid_alive_and_owned(pid, &entry.path) {
        return Err(format!("sandbox pid {pid} is still alive; refusing purge"));
    }
    fs::remove_dir_all(&entry.path)
        .map(|_| true)
        .map_err(|e| e.to_string())
}

/// State dir is per backend instance: two backend processes can never
/// restore each other's transcripts.
fn instance_state_dir() -> Result<PathBuf, Error> {
    let dir = std::env::temp_dir().join(format!("susutaku-agent-state-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn purge_stale_state() {
    let own_pid = std::process::id();
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.strip_prefix("susutaku-agent-state-"))
            .and_then(|p| p.parse::<u32>().ok())
        else {
            continue;
        };
        if pid != own_pid && !pid_alive(pid) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sandbox_root(sandbox_id: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{SANDBOX_PREFIX}{sandbox_id}"))
}

fn create_workspace_layout(root: &Path) -> Result<(), Error> {
    fs::create_dir_all(root.join(WORKSPACE_MOUNT))?;
    fs::create_dir_all(root.join(ROOTFS_SUBDIR))?;
    Ok(())
}

fn write_metadata(root: &Path, sandbox_id: &str) -> Result<(), Error> {
    let meta = SandboxMetadata {
        sandbox_id: sandbox_id.to_owned(),
        owner_pid: std::process::id(),
        created_at_unix: unix_now(),
    };
    let raw = serde_json::to_string(&meta).map_err(|e| Error::other(e.to_string()))?;
    fs::write(root.join(METADATA_FILE), raw)
}

fn random_hex_id() -> Result<String, Error> {
    let mut bytes = [0u8; ID_BYTES];
    fs::File::open(URANDOM)?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn path_to_cstring(p: &Path) -> Result<CString, Error> {
    CString::new(p.as_os_str().as_encoded_bytes()).map_err(|e| Error::other(e.to_string()))
}

/// Join a base path with a fixed internal name; suffixes are compile-time
/// constants without interior NULs, so construction cannot fail.
fn join_cstring(base: &CString, suffix: &str) -> CString {
    let mut s = base.to_bytes().to_vec();
    s.pop(); // drop NUL
    s.push(b'/');
    s.extend_from_slice(suffix.as_bytes());
    CString::new(s).unwrap_or_default()
}

fn inheritable_pipe() -> Result<(i32, i32), Error> {
    let mut fds = [0 as libc::c_int; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(Error::last_os_error());
    }
    Ok((fds[0], fds[1]))
}

fn cloexec_pipe() -> Result<(i32, i32), Error> {
    let mut fds = [0 as libc::c_int; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(Error::last_os_error());
    }
    Ok((fds[0], fds[1]))
}

fn drain<R: Read>(rd: &mut R, cap: usize, into: &mut Vec<u8>) {
    let mut buf = [0u8; 8192];
    let mut dropped = false;
    loop {
        match rd.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if dropped {
                    continue;
                }
                into.extend_from_slice(&buf[..n]);
                if into.len() >= cap.saturating_mul(OUTPUT_HEADROOM) {
                    dropped = true;
                }
            }
        }
    }
}

fn truncate_bytes(text: &mut String, max_bytes: usize) {
    if text.len() <= max_bytes {
        return;
    }
    let mut cut = max_bytes;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text.truncate(cut);
}

/// Host-side resolution of the stored relative cwd. Kernel namespaces are
/// the real boundary; this keeps stored state meaningful and canonical.
fn resolve_cwd(root: &Path, rel: &str) -> PathBuf {
    let workspace = root.join(WORKSPACE_MOUNT);
    let rel = rel.trim();
    if rel.is_empty() || rel == "." || Path::new(rel).is_absolute() {
        return workspace;
    }
    let safe = Path::new(rel)
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if !safe {
        return workspace;
    }
    match workspace.join(rel).canonicalize() {
        Ok(canon) if canon.starts_with(&workspace) && canon.is_dir() => canon,
        _ => workspace,
    }
}

/// Map the stored relative cwd to its path inside the sandbox mount tree.
fn cwd_mount_path(root: &Path, rel: &str) -> String {
    let _ = root;
    let rel = rel.trim();
    if rel.is_empty() || rel == "." {
        "/workspace".to_string()
    } else {
        format!("/workspace/{}", rel.trim_start_matches('/'))
    }
}

fn pid_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn process_start_time(pid: u32) -> Option<std::time::SystemTime> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // starttime is field 22; fields after `comm` may contain spaces/parens,
    // so parse after the final ')'.
    let rest = stat.rsplit(')').next()?;
    let ticks: u64 = rest.split_whitespace().nth(19)?.parse().ok()?;
    const CLK_TCK: u64 = 100;
    Some(boot_time()? + Duration::from_secs(ticks / CLK_TCK))
}

fn boot_time() -> Option<std::time::SystemTime> {
    let stat = fs::read_to_string("/proc/stat").ok()?;
    let line = stat.lines().find(|l| l.starts_with("btime"))?;
    let secs: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(std::time::UNIX_EPOCH + Duration::from_secs(secs))
}

fn dir_create_time(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).and_then(|m| m.created()).ok()
}
