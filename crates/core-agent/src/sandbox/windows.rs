//! Windows sandbox: restricted token + Job Object + ACLed workspace.
//!
//! # Security model
//!
//! ## Protects against
//! - Privilege inheritance: the child runs under a token derived via
//!   `CreateRestrictedToken` with **all privileges disabled** (no
//!   `SeDebugPrivilege`, `SeBackupPrivilege`, `SeRestorePrivilege`,
//!   `SeShutdownPrivilege`, `SeTakeOwnershipPrivilege`,
//!   `SeCreateSymbolicLinkPrivilege`, `SeLoadDriverPrivilege`, ...) and a
//!   **restricted SID list containing only the process logon SID**. Any
//!   access check must succeed against that restricted list, so objects that
//!   grant access to the user SID / `Authenticated Users` / `Users` but not
//!   to the logon SID are denied.
//! - Filesystem escape: the workspace DACL is replaced (protected DACL, no
//!   inherited ACEs) granting full control only to the logon SID plus
//!   SYSTEM/Administrators. Combined with the restricted token this blocks
//!   reads/writes under `C:\Users\...` (outside the workspace), the Windows
//!   system directories, backend config/secrets, and other temp dirs.
//! - Symlink/junction escape: every ancestor of the workspace is checked for
//!   reparse points before each run, and the sandbox cannot create new
//!   symlinks (privilege disabled).
//! - Process-tree escape: PowerShell and all descendants run inside a Job
//!   Object with `KILL_ON_JOB_CLOSE`; closing the job handle (normal
//!   completion, timeout, purge, drop, panic) terminates the whole tree.
//! - Resource exhaustion: max processes, per-process and job memory limits,
//!   CPU-rate hard cap, wall-clock timeout, stdout/stderr caps.
//! - Stale-sandbox misidentification (PID reuse): dirs carry a unique id
//!   marker; purge compares dir creation time to the process start time.
//!
//! ## Does NOT protect against
//! - **Network access.** A restricted token still has a working TCP/IP stack;
//!   `allow_network` is an explicit flag but enforcement is best-effort. For
//!   a hard block use Windows Firewall rules scoped to the sandboxed
//!   `powershell.exe`, or port this module to an AppContainer (recommended
//!   follow-up).
//! - Windows kernel exploits or a compromised `powershell.exe` binary.
//! - PowerShell-specific attack surface (AMSI bypass, .NET reflection):
//!   `-NoProfile -NonInteractive` reduces surface but is NOT a sandbox. The
//!   security boundary is token + job + DACLs, not the PowerShell flags.
//! - Objects granting access to `Everyone` (e.g. parts of `C:\` root).
//! - Disk-fill DoS inside the workspace, memory bandwidth, leftover CPU
//!   within the rate cap.
//!
//! ## Required Windows configuration
//! - Run the backend **unelevated** (a normal user session). If it runs as
//!   Administrator or LocalSystem the restricted token still drops most
//!   rights, but the design assumes an unelevated parent; do not run the
//!   backend elevated.
//! - No services or registry changes required. Optional network hardening:
//!   firewall rule blocking outbound traffic for
//!   `WindowsPowerShell\v1.0\powershell.exe` when `allow_network == false`.
//!
//! Architecture:
//! ```text
//! backend (unelevated)
//!   └─ CreateRestrictedToken(DisableAllPrivileges, restrict=[logon SID])
//!        └─ CreateProcessAsUserW → powershell -NoProfile -NonInteractive
//!             └─ Job Object (kill-on-close, proc/mem/CPU limits)
//!                  └─ child processes (stay inside the job)
//! ```

use std::fs;
use std::io::Error;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use windows::Win32::Foundation::{
    CloseHandle, FILETIME, HANDLE, HANDLE_FLAG_INHERIT, HANDLE_FLAGS, HLOCAL, LocalFree,
    SetHandleInformation, WAIT_OBJECT_0, WIN32_ERROR,
};
use windows::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_W, GRANT_ACCESS, NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT, SetEntriesInAclW,
    SetNamedSecurityInfoW, TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
};
use windows::Win32::Security::{
    ACL, CREATE_RESTRICTED_TOKEN_FLAGS, CreateRestrictedToken, DACL_SECURITY_INFORMATION,
    GetTokenInformation, PROTECTED_DACL_SECURITY_INFORMATION, SECURITY_ATTRIBUTES,
    SID_AND_ATTRIBUTES, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_GROUPS, TOKEN_QUERY,
    TokenGroups,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT, FILE_SHARE_NONE,
    GetFileAttributesW, OPEN_EXISTING, ReadFile,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_CPU_RATE_CONTROL_ENABLE,
    JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
    JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION, JOB_OBJECT_LIMIT_JOB_MEMORY,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    JOBOBJECT_CPU_RATE_CONTROL_INFORMATION, JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectCpuRateControlInformation,
    JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW, GetCurrentProcess,
    GetExitCodeProcess, GetProcessTimes, INFINITE, OpenProcess, OpenProcessToken,
    PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, ResumeThread, STARTF_USESTDHANDLES,
    STARTUPINFOW, TerminateProcess, WaitForSingleObject,
};
use windows::core::{PCWSTR, PWSTR};

pub const SHELL: &str = "powershell";
pub const RUN_FLAG: &str = "-Command";
pub const MAX_OUTPUT_BYTES: usize = 1 << 20;
pub const MAX_HISTORY: usize = 128;
pub const SANDBOX_PREFIX: &str = "susutaku-agent-sandbox-";
pub const STATE_FILE: &str = "agent-sandbox-state.json";
pub const TASKLIST: &str = "tasklist";
pub const TASKLIST_ARG_FILTER: &str = "/FI";
pub const TASKLIST_ARG_NOHEADER: &str = "/NH";

const POWERSHELL_REL: &str = r"WindowsPowerShell\v1.0\powershell.exe";
const PS_FLAGS: &str = "-NoProfile -NonInteractive -NoLogo";
const WSL_EXE: &str = "wsl.exe";
const WSL_STATUS_FLAG: &str = "--status";
const WSL_RUN_FLAG: &str = "-e";
const WSL_SHELL: &str = "/bin/sh";
const WSL_SHELL_FLAG: &str = "-c";
const WSL_MNT_ROOT: &str = "/mnt/";
const WSL_CD: &str = "cd";
const SYSTEM32_REL: &str = "System32";
const WIN_ROOT_COLON: char = ':';
const SANDBOX_ID_FILE: &str = "sandbox-id";
const WORKSPACE_TMP: &str = "tmp";
const SE_GROUP_LOGON_ID_MASK: u32 = 0xC000_0000;
const INVALID_FILE_ATTRIBUTES: u32 = u32::MAX;
const ACE_INHERIT_MASK: u32 = 0x3; // CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE
const GENERIC_ALL_MASK: u32 = 0x1000_0000;
const FILE_GENERIC_READ_MASK: u32 = 0x0012_0089;

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_MAX_PROCESSES: u32 = 16;
const DEFAULT_MEMORY_MB: u64 = 512;
const DEFAULT_CPU_RATE_PERCENT: u32 = 50;
const MEM_PER_MB: u64 = 1024 * 1024;
const MIN_MEMORY_MB: u64 = 16;
const MAX_MEMORY_MB: u64 = 4096;
const STDERR_REPORT_CAP: usize = 4096;
const PIPE_READ_BUF: usize = 8192;

pub use crate::sandbox_abstract_layer::{
    Guarantee, HistoryEntry, Role, SandboxLayer, SandboxState,
};

/// Execution backend for sandboxed commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsExec {
    /// Native hardening: powershell under restricted token + Job Object.
    RestrictedToken,
    /// Commands run via `wsl.exe -e /bin/sh -c ...` inside WSL; the
    /// `wsl.exe` launch itself still runs under the restricted token + job.
    Wsl,
}

/// Default execution mode when detection has not picked `Wsl`.
pub const DEFAULT_EXEC: WindowsExec = WindowsExec::RestrictedToken;

static EXEC_CACHE: OnceLock<WindowsExec> = OnceLock::new();

/// Pick the execution mode: `Wsl` when `wsl.exe --status` succeeds,
/// otherwise [`DEFAULT_EXEC`]. Result is cached for the process lifetime.
pub fn detect() -> WindowsExec {
    *EXEC_CACHE.get_or_init(|| {
        let ok = Command::new(WSL_EXE)
            .arg(WSL_STATUS_FLAG)
            .output()
            .is_ok_and(|o| o.status.success());
        if ok { WindowsExec::Wsl } else { DEFAULT_EXEC }
    })
}

/// Translate `C:\a\b` to `/mnt/c/a/b`; pass through anything else.
pub fn windows_path_to_wsl(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let (drive, rest) = match text.split_once(WIN_ROOT_COLON) {
        Some((d, r)) if d.len() == 1 && d.chars().all(|c| c.is_ascii_alphabetic()) => (d, r),
        _ => return text,
    };
    format!(
        "{WSL_MNT_ROOT}{}{rest}",
        drive
            .chars()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
    )
}

/// Windows backend: restricted token + Job Object + protected DACL.
/// Kernel-enforced for filesystem/process/resources; network access is
/// best-effort only (see `Guarantee::BestEffort` docs in the abstract layer).
impl SandboxLayer for Sandbox {
    fn guarantee(&self) -> Guarantee {
        Guarantee::BestEffort
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

/// Free-function run inside a one-off sandbox with default limits (API compat).
/// Resource limits for one sandboxed run, enforced by the Job Object and the
/// run harness.
#[derive(Debug, Clone)]
pub struct SandboxLimits {
    pub timeout_secs: u64,
    pub max_output_bytes: usize,
    pub max_processes: u32,
    pub memory_mb: u64,
    /// Hard CPU-rate cap for the whole job, percent 1..=100.
    pub cpu_rate_percent: u32,
    /// Explicit network opt-in. Enforcement is best-effort (module docs).
    pub allow_network: bool,
}

impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            max_output_bytes: MAX_OUTPUT_BYTES,
            max_processes: DEFAULT_MAX_PROCESSES,
            memory_mb: DEFAULT_MEMORY_MB,
            cpu_rate_percent: DEFAULT_CPU_RATE_PERCENT,
            allow_network: false,
        }
    }
}

/// Free-function run inside a one-off sandbox with default limits (API compat).
pub fn run(cmd: &str) -> Result<String, Error> {
    let sb = Sandbox::new()?;
    let out = sb.run(cmd)?;
    sb.purge();
    Ok(out)
}

pub struct Sandbox {
    root: PathBuf,
    sandbox_id: String,
    state_path: PathBuf,
    state: RwLock<SandboxState>,
    purged: AtomicBool,
}

impl Sandbox {
    pub fn new() -> Result<Self, Error> {
        purge_stale_dirs();
        let sandbox_id = new_sandbox_id();
        let root = sandbox_root(&sandbox_id);
        create_isolated_workspace(&root)?;
        write_marker(&root, &sandbox_id)?;
        let state_path = state_path();
        let state = RwLock::new(SandboxState {
            cwd: ".".into(),
            history: Vec::new(),
        });
        Ok(Self {
            root,
            sandbox_id,
            state_path,
            state,
            purged: AtomicBool::new(false),
        })
    }

    /// Sandbox rooted at an explicit work tree (one per agent), with a
    /// state file scoped to that tree.
    pub fn new_in(work_tree: &Path) -> Result<Self, Error> {
        fs::create_dir_all(work_tree)?;
        let name = work_tree
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let sandbox_id = new_sandbox_id();
        let root = work_tree.join("sandbox");
        create_isolated_workspace(&root)?;
        write_marker(&root, &sandbox_id)?;
        let state_path = std::env::temp_dir().join(format!("{STATE_FILE}.{name}"));
        let state = RwLock::new(SandboxState {
            cwd: ".".into(),
            history: Vec::new(),
        });
        Ok(Self {
            root,
            sandbox_id,
            state_path,
            state,
            purged: AtomicBool::new(false),
        })
    }

    /// Reload state from the JSON file and recreate the sandbox dir.
    pub fn restore() -> Result<Self, Error> {
        let state_path = state_path();
        let state: SandboxState = match fs::read_to_string(&state_path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => SandboxState::default(),
        };
        let sandbox_id = new_sandbox_id();
        let root = sandbox_root(&sandbox_id);
        create_isolated_workspace(&root)?;
        write_marker(&root, &sandbox_id)?;
        Ok(Self {
            root,
            sandbox_id,
            state_path,
            state: RwLock::new(state),
            purged: AtomicBool::new(false),
        })
    }

    /// Remove leftover state of dead processes (call at backend startup).
    pub fn purge_stale() {
        let _ = fs::remove_file(state_path());
        purge_stale_dirs();
    }

    /// Workspace root the sandboxed commands run in.
    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }

    /// Unique per-sandbox identifier (mirrored in the dir's marker file).
    pub fn id(&self) -> &str {
        &self.sandbox_id
    }

    /// Terminate the process tree (jobs die on job-handle close), delete the
    /// workspace and saved state.
    pub fn purge(&self) {
        self.clear();
        let _ = fs::remove_file(&self.state_path);
    }

    pub fn run(&self, cmd: &str) -> Result<String, Error> {
        self.run_with_limits(cmd, &SandboxLimits::default())
    }

    pub fn run_with_limits(&self, cmd: &str, limits: &SandboxLimits) -> Result<String, Error> {
        if self.purged.load(Ordering::Acquire) {
            return Err(Error::other("sandbox already purged"));
        }
        ensure_no_reparse_points(&self.root)?;
        let cwd = resolve_cwd(&self.root, &self.cwd_snapshot());
        let out = spawn_restricted(detect(), cmd, &cwd, limits)?;
        let text = out.stdout_text(limits.max_output_bytes);
        self.record(HistoryEntry {
            role: Role::Tool,
            content: format!("$ {cmd}\n{text}"),
        });
        out.into_result(text)
    }

    pub fn push_context(&self, role: Role, content: impl Into<String>) {
        self.record(HistoryEntry {
            role,
            content: content.into(),
        });
    }

    pub fn transcript(&self) -> Vec<HistoryEntry> {
        self.state
            .read()
            .map(|s| s.history.clone())
            .unwrap_or_default()
    }

    pub fn set_cwd(&self, rel: &str) -> Result<(), Error> {
        resolve_cwd(&self.root, rel);
        if let Ok(mut s) = self.state.write() {
            s.cwd = rel.into();
        }
        self.save()
    }

    fn cwd_snapshot(&self) -> String {
        self.state
            .read()
            .map(|s| s.cwd.clone())
            .unwrap_or_else(|_| ".".into())
    }

    fn record(&self, entry: HistoryEntry) {
        if let Ok(mut s) = self.state.write() {
            s.history.push(entry);
            let excess = s.history.len().saturating_sub(MAX_HISTORY);
            s.history.drain(..excess);
        }
        // Transcript loss is degraded service, not a security boundary.
        let _ = self.save();
    }

    fn save(&self) -> Result<(), Error> {
        let snapshot = self
            .state
            .read()
            .map_err(|e| Error::other(e.to_string()))?
            .clone();
        let raw =
            serde_json::to_string_pretty(&snapshot).map_err(|e| Error::other(e.to_string()))?;
        fs::write(&self.state_path, raw)
    }

    /// Best-effort teardown used by both `purge()` and `Drop`.
    fn clear(&self) {
        self.purged.store(true, Ordering::Release);
        let _ = fs::remove_dir_all(&self.root).or_else(|_| fs::remove_dir_all(&self.root));
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        self.clear();
    }
}

pub fn state_path() -> PathBuf {
    std::env::temp_dir().join(STATE_FILE)
}

// ---------------------------------------------------------------------------
// Sandboxed process execution
// ---------------------------------------------------------------------------

struct RunOutput {
    status_success: bool,
    stderr: String,
    stdout: String,
}

impl RunOutput {
    fn stdout_text(&self, cap: usize) -> String {
        let mut text = self.stdout.clone();
        truncate_bytes(&mut text, cap);
        text
    }

    fn into_result(self, text: String) -> Result<String, Error> {
        if self.status_success {
            Ok(text)
        } else {
            let mut msg = self.stderr;
            truncate_bytes(&mut msg, STDERR_REPORT_CAP);
            Err(Error::other(msg))
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

/// Run `cmd` under a restricted token inside a fresh Job Object, bounded by
/// `limits`. Every error path terminates the job, so the process tree never
/// outlives this function.
fn build_command(mode: WindowsExec, cmd: &str, cwd: &Path) -> (String, String) {
    match mode {
        WindowsExec::RestrictedToken => {
            let exe = system_root()
                .join(POWERSHELL_REL)
                .to_string_lossy()
                .into_owned();
            let cmdline = format!("\"{exe}\" {PS_FLAGS} {RUN_FLAG} {}", ps_quote(cmd));
            (exe, cmdline)
        }
        WindowsExec::Wsl => {
            let exe = system_root()
                .join(SYSTEM32_REL)
                .join(WSL_EXE)
                .to_string_lossy()
                .into_owned();
            let remote = format!("{WSL_CD} '{}' && {}", windows_path_to_wsl(cwd), cmd);
            let cmdline = format!(
                "\"{exe}\" {WSL_RUN_FLAG} {WSL_SHELL} {WSL_SHELL_FLAG} {}",
                sh_quote(&remote)
            );
            (exe, cmdline)
        }
    }
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn spawn_restricted(
    mode: WindowsExec,
    cmd: &str,
    cwd: &Path,
    limits: &SandboxLimits,
) -> Result<RunOutput, Error> {
    let token = restricted_token()?;
    let token_guard = HandleGuard(token);
    let job = create_limited_job(limits)?;

    let (_exe, cmdline) = build_command(mode, cmd, cwd);
    let mut cmdline_w = to_wide(&cmdline);
    let cwd_w = to_wide(&cwd.to_string_lossy());
    let env = sandbox_env(cwd);

    let (stdout_rd, stdout_wr) = inheritable_pipe()?;
    let (stderr_rd, stderr_wr) = inheritable_pipe()?;
    let nul = open_nul()?;

    let si = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        dwFlags: STARTF_USESTDHANDLES,
        hStdInput: nul,
        hStdOutput: stdout_wr,
        hStdError: stderr_wr,
        ..Default::default()
    };

    let mut pi = PROCESS_INFORMATION::default();
    let created = unsafe {
        CreateProcessAsUserW(
            Some(token),
            PCWSTR::null(),
            Some(PWSTR(cmdline_w.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
            Some(env.as_ptr().cast()),
            PCWSTR(cwd_w.as_ptr()),
            &si,
            &mut pi,
        )
    };
    if created.is_err() {
        // No process was created; release the unused job handle.
        unsafe {
            let _ = CloseHandle(job);
        }
        return Err(Error::last_os_error());
    }

    struct JobGuard(HANDLE);
    impl Drop for JobGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = TerminateJobObject(self.0, 1);
                let _ = CloseHandle(self.0);
            }
        }
    }

    if unsafe { AssignProcessToJobObject(job, pi.hProcess) }.is_err() {
        unsafe {
            let _ = TerminateProcess(pi.hProcess, 1);
            let _ = CloseHandle(pi.hThread);
            let _ = CloseHandle(pi.hProcess);
            let _ = CloseHandle(job);
        }
        return Err(Error::last_os_error());
    }
    let guard = JobGuard(job);

    if unsafe { ResumeThread(pi.hThread) } == u32::MAX {
        return Err(Error::last_os_error());
    }
    unsafe {
        let _ = CloseHandle(pi.hThread);
    }

    let out_reader = spawn_reader(stdout_rd.0 as usize);
    let err_reader = spawn_reader(stderr_rd.0 as usize);

    let timeout_ms = limits
        .timeout_secs
        .saturating_mul(1000)
        .min(INFINITE as u64) as u32;
    let wait = unsafe { WaitForSingleObject(pi.hProcess, timeout_ms) };
    if wait != WAIT_OBJECT_0 {
        // guard drop terminates the whole tree on return.
        return Err(Error::other(format!(
            "sandbox command timed out after {}s",
            limits.timeout_secs
        )));
    }

    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();

    let mut exit_code = 0u32;
    let status_success =
        unsafe { GetExitCodeProcess(pi.hProcess, &mut exit_code) }.is_ok() && exit_code == 0;
    unsafe {
        let _ = CloseHandle(pi.hProcess);
    }
    drop(guard);
    drop(token_guard);

    Ok(RunOutput {
        status_success,
        stderr,
        stdout,
    })
}

/// Raw HANDLE is not `Send`; the pipe read end is exclusively owned by the
/// reader thread from spawn until close, so moving the address is sound.
fn spawn_reader(raw: usize) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let pipe = HANDLE(raw as *mut core::ffi::c_void);
        let mut buf = [0u8; PIPE_READ_BUF];
        let mut text = String::new();
        let mut dropped = false;
        loop {
            let mut got = 0u32;
            if unsafe { ReadFile(pipe, Some(&mut buf), Some(&mut got), None) }.is_err() || got == 0
            {
                break;
            }
            // Keep draining so the child never blocks on a full pipe; only
            // the leading portion is kept.
            if dropped {
                continue;
            }
            text.push_str(&String::from_utf8_lossy(&buf[..got as usize]));
            if text.len() >= MAX_OUTPUT_BYTES.saturating_mul(2) {
                dropped = true;
            }
        }
        unsafe {
            let _ = CloseHandle(pipe);
        }
        text
    })
}

fn create_limited_job(limits: &SandboxLimits) -> Result<HANDLE, Error> {
    let job = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
        .map_err(|e| Error::other(e.to_string()))?;
    // Fail closed: a job whose limits could not be configured is never used.
    if configure_job_limits(job, limits).is_err() {
        unsafe {
            let _ = CloseHandle(job);
        }
        return Err(Error::other("failed to configure job object limits"));
    }
    Ok(job)
}

fn configure_job_limits(job: HANDLE, limits: &SandboxLimits) -> Result<(), Error> {
    let mut ext = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    ext.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION
        | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
        | JOB_OBJECT_LIMIT_PROCESS_MEMORY
        | JOB_OBJECT_LIMIT_JOB_MEMORY;
    ext.BasicLimitInformation.ActiveProcessLimit = limits.max_processes.max(1);
    let mem = limits
        .memory_mb
        .clamp(MIN_MEMORY_MB, MAX_MEMORY_MB)
        .saturating_mul(MEM_PER_MB);
    let mem = usize::try_from(mem).unwrap_or(usize::MAX);
    ext.ProcessMemoryLimit = mem;
    ext.JobMemoryLimit = mem;

    unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &ext as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    }
    .map_err(|e| Error::other(e.to_string()))?;

    let cpu = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION {
        ControlFlags: JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
        Anonymous: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0 {
            CpuRate: limits.cpu_rate_percent.clamp(1, 100) * 100,
        },
    };
    unsafe {
        SetInformationJobObject(
            job,
            JobObjectCpuRateControlInformation,
            &cpu as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
        )
    }
    .map_err(|e| Error::other(e.to_string()))?;

    Ok(())
}

/// Token with all privileges disabled and restricted SIDs = [logon SID].
fn restricted_token() -> Result<HANDLE, Error> {
    let mut token = HANDLE::default();
    unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY,
            &mut token,
        )
    }
    .map_err(|e| Error::other(e.to_string()))?;

    let logon = match logon_sid(token) {
        Ok(s) => s,
        Err(e) => {
            unsafe {
                let _ = CloseHandle(token);
            }
            return Err(e);
        }
    };

    let mut restricted = HANDLE::default();
    let result = unsafe {
        // DISABLE_SANDBOX = 1 disables all privileges in the new token.
        CreateRestrictedToken(
            token,
            CREATE_RESTRICTED_TOKEN_FLAGS(1),
            None,
            None,
            Some(&[logon]),
            &mut restricted,
        )
    };
    unsafe {
        let _ = CloseHandle(token);
    }
    result.map_err(|e| Error::other(e.to_string()))?;
    Ok(restricted)
}

struct HandleGuard(HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn logon_sid(token: HANDLE) -> Result<SID_AND_ATTRIBUTES, Error> {
    let mut len = 0u32;
    // Probe for the required buffer size.
    let _ = unsafe { GetTokenInformation(token, TokenGroups, None, 0, &mut len) };
    if len == 0 {
        return Err(Error::last_os_error());
    }
    let mut buf = vec![0u8; len as usize];
    unsafe {
        GetTokenInformation(
            token,
            TokenGroups,
            Some(buf.as_mut_ptr().cast()),
            len,
            &mut len,
        )
    }
    .map_err(|e| Error::other(e.to_string()))?;
    let groups = unsafe { &*(buf.as_ptr() as *const TOKEN_GROUPS) };
    for i in 0..groups.GroupCount as usize {
        let g = unsafe { *groups.Groups.as_ptr().add(i) };
        if g.Attributes & SE_GROUP_LOGON_ID_MASK == SE_GROUP_LOGON_ID_MASK {
            return Ok(g);
        }
    }
    Err(Error::other("no logon SID in process token"))
}

// ---------------------------------------------------------------------------
// Workspace + ACLs
// ---------------------------------------------------------------------------

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

fn new_sandbox_id() -> String {
    let n = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos:x}-{n:x}", std::process::id())
}

fn sandbox_root(sandbox_id: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{SANDBOX_PREFIX}{sandbox_id}"))
}

fn write_marker(root: &Path, sandbox_id: &str) -> Result<(), Error> {
    fs::write(root.join(SANDBOX_ID_FILE), sandbox_id)
}

/// Create the workspace and immediately lock its DACL down. A failed ACL
/// replacement removes the directory: an open-DACL workspace is never used.
fn create_isolated_workspace(root: &Path) -> Result<(), Error> {
    fs::create_dir_all(root)?;
    fs::create_dir_all(root.join(WORKSPACE_TMP))?;
    if let Err(e) = restrict_workspace_dacl(root) {
        let _ = fs::remove_dir_all(root);
        return Err(Error::other(format!("workspace ACL hardening failed: {e}")));
    }
    Ok(())
}

/// Replace the workspace DACL with a protected DACL: full control for the
/// logon SID (inheriting) plus SYSTEM/Administrators, inherited ACEs stripped.
fn restrict_workspace_dacl(root: &Path) -> Result<(), Error> {
    let logon = logon_sid_of_current_process()?;
    let ea = EXPLICIT_ACCESS_W {
        grfAccessPermissions: GENERIC_ALL_MASK,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: windows::Win32::Security::ACE_FLAGS(ACE_INHERIT_MASK),
        Trustee: TRUSTEE_W {
            pMultipleTrustee: std::ptr::null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_USER,
            ptstrName: PWSTR(logon.Sid.0.cast()),
        },
    };

    let mut acl_ptr: *mut ACL = std::ptr::null_mut();
    let acl_err = unsafe { SetEntriesInAclW(Some(&[ea]), None, &mut acl_ptr) };
    if acl_err != WIN32_ERROR(0) {
        return Err(Error::other(format!(
            "SetEntriesInAclW failed: {}",
            acl_err.0
        )));
    }

    let path_w = to_wide(&root.to_string_lossy());
    let result = unsafe {
        SetNamedSecurityInfoW(
            PCWSTR(path_w.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(acl_ptr),
            None,
        )
    };
    unsafe {
        LocalFree(Some(HLOCAL(acl_ptr.cast())));
    }
    if result != WIN32_ERROR(0) {
        return Err(Error::other(format!(
            "SetNamedSecurityInfoW failed: {}",
            result.0
        )));
    }
    Ok(())
}

fn logon_sid_of_current_process() -> Result<SID_AND_ATTRIBUTES, Error> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
        .map_err(|e| Error::other(e.to_string()))?;
    let res = logon_sid(token);
    unsafe {
        let _ = CloseHandle(token);
    }
    res
}

/// Refuse to run when any existing ancestor of `root` is a reparse point
/// (symlink / junction / mount), preventing path redirection out of the
/// workspace.
fn ensure_no_reparse_points(root: &Path) -> Result<(), Error> {
    for anc in root.ancestors() {
        if !anc.exists() {
            break;
        }
        let attrs = unsafe { GetFileAttributesW(PCWSTR(to_wide(&anc.to_string_lossy()).as_ptr())) };
        if attrs != INVALID_FILE_ATTRIBUTES && attrs & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
            return Err(Error::other(format!(
                "refusing to run: reparse point in sandbox path: {}",
                anc.display()
            )));
        }
    }
    Ok(())
}

/// Resolve a relative stored cwd strictly inside `root`. Rejects absolute
/// paths, `..`, and anything canonicalizing outside the workspace.
fn resolve_cwd(root: &Path, rel: &str) -> PathBuf {
    let rel = rel.trim();
    if rel.is_empty() || Path::new(rel).is_absolute() {
        return root.to_path_buf();
    }
    let safe_components = Path::new(rel)
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if !safe_components {
        return root.to_path_buf();
    }
    match root.join(rel).canonicalize() {
        Ok(canon) if canon.starts_with(root) && canon.is_dir() => canon,
        _ => root.to_path_buf(),
    }
}

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

/// Minimal environment so backend secrets can never leak via inheritance.
/// Only what PowerShell needs to boot.
fn sandbox_env(cwd: &Path) -> Vec<u16> {
    let sysroot = system_root().to_string_lossy().into_owned();
    let tmp = cwd.join(WORKSPACE_TMP);
    let pairs = [
        ("SystemRoot", sysroot.clone()),
        ("SystemDrive", "C:".to_string()),
        ("PATH", format!("{sysroot}\\System32;{sysroot}")),
        ("PATHEXT", ".COM;.EXE;.BAT;.CMD".to_string()),
        ("TEMP", tmp.to_string_lossy().into_owned()),
        ("TMP", tmp.to_string_lossy().into_owned()),
        ("COMPUTERNAME", "sandbox".to_string()),
        ("SESSIONNAME", "sandbox".to_string()),
    ];
    let mut block = Vec::new();
    for (k, v) in pairs {
        block.extend(format!("{k}={v}").encode_utf16());
        block.push(0);
    }
    block.push(0);
    block
}

fn system_root() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Quote a value for the Windows command line (PowerShell `-Command` arg).
fn ps_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c == '"' {
            out.push_str("\"\"");
        } else {
            out.push(c);
        }
    }
    out.push('"');
    out
}

fn inheritable_pipe() -> Result<(HANDLE, HANDLE), Error> {
    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        bInheritHandle: true.into(),
        ..Default::default()
    };
    let mut rd = HANDLE::default();
    let mut wr = HANDLE::default();
    unsafe { CreatePipe(&mut rd, &mut wr, Some(&sa), 0) }
        .map_err(|e| Error::other(e.to_string()))?;
    // The parent's read end must not leak into the child.
    unsafe { SetHandleInformation(rd, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0)) }
        .map_err(|e| Error::other(e.to_string()))?;
    Ok((rd, wr))
}

fn open_nul() -> Result<HANDLE, Error> {
    unsafe {
        CreateFileW(
            PCWSTR(to_wide("NUL").as_ptr()),
            FILE_GENERIC_READ_MASK,
            FILE_SHARE_NONE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|e| Error::other(e.to_string()))
}

// ---------------------------------------------------------------------------
// Stale-sandbox management
// ---------------------------------------------------------------------------

/// Sandbox dirs in TMPDIR with owning-process liveness and id marker.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SandboxDir {
    pub pid: u32,
    pub alive: bool,
    pub path: PathBuf,
    /// Content of the dir's `sandbox-id` marker, when present.
    pub sandbox_id: Option<String>,
}

pub fn list_dirs() -> Vec<SandboxDir> {
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return Vec::new();
    };
    let mut dirs: Vec<SandboxDir> = entries
        .flatten()
        .filter_map(|entry| {
            let pid = entry
                .file_name()
                .into_string()
                .ok()
                .and_then(|n| parse_owner_pid(&n))?;
            let sandbox_id = dir_marker(&entry.path());
            Some(SandboxDir {
                pid,
                alive: pid_alive_and_fresh(pid, &entry.path()),
                path: entry.path(),
                sandbox_id,
            })
        })
        .collect();
    dirs.sort_by_key(|d| d.path.clone());
    dirs
}

/// `SANDBOX_PREFIX<pid>-<unique>` (or legacy `<pid>`) → owning pid.
fn parse_owner_pid(name: &str) -> Option<u32> {
    let rest = name.strip_prefix(SANDBOX_PREFIX)?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn dir_marker(path: &Path) -> Option<String> {
    fs::read_to_string(path.join(SANDBOX_ID_FILE))
        .ok()
        .map(|s| s.trim().to_owned())
}

/// Remove sandbox dirs of dead owners. PID-reuse guard: a dir created *before*
/// the current start time of that PID belongs to a recycled process and is
/// treated as stale even though the PID looks alive.
fn purge_stale_dirs() {
    let own_pid = std::process::id();
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .into_string()
            .ok()
            .and_then(|n| parse_owner_pid(&n))
        else {
            continue;
        };
        if pid == own_pid {
            continue;
        }
        if !pid_alive_and_fresh(pid, &entry.path()) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// Delete the sandbox dir owned by `pid`. Refuses the caller's own sandbox
/// and any dir whose owner process is still alive and plausibly the dir's
/// creator (see `pid_alive_and_fresh`).
pub fn purge_dir(pid: u32) -> Result<bool, String> {
    if pid == std::process::id() {
        return Err("cannot purge the running backend's own sandbox".into());
    }
    let Some(path) = find_dir_for_pid(pid) else {
        return Ok(false);
    };
    if pid_alive_and_fresh(pid, &path) {
        return Err(format!("sandbox pid {pid} is still alive; refusing purge"));
    }
    fs::remove_dir_all(&path)
        .map(|_| true)
        .map_err(|e| e.to_string())
}

fn find_dir_for_pid(pid: u32) -> Option<PathBuf> {
    let entries = fs::read_dir(std::env::temp_dir()).ok()?;
    let exact = format!("{SANDBOX_PREFIX}{pid}");
    entries
        .flatten()
        .find(|e| {
            e.file_name()
                .to_str()
                .map(|n| n == exact || n.starts_with(&format!("{exact}-")))
                .unwrap_or(false)
        })
        .map(|e| e.path())
}

/// Liveness + PID-reuse freshness: true only when a process with this PID
/// exists AND the sandbox dir was created after that process started (so the
/// dir plausibly belongs to it). On inconclusive evidence we treat the dir as
/// owned — never delete on doubt.
fn pid_alive_and_fresh(pid: u32, dir: &Path) -> bool {
    if !pid_alive(pid) {
        return false;
    }
    match (process_start_time(pid), fs_create_time(dir)) {
        (Some(p_start), Some(d_created)) => d_created >= p_start,
        _ => true,
    }
}

fn process_start_time(pid: u32) -> Option<std::time::SystemTime> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()? };
    let mut created = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let ok =
        unsafe { GetProcessTimes(handle, &mut created, &mut exit, &mut kernel, &mut user) }.is_ok();
    unsafe {
        let _ = CloseHandle(handle);
    }
    if ok {
        filetime_to_system(created)
    } else {
        None
    }
}

fn fs_create_time(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).and_then(|m| m.created()).ok()
}

fn filetime_to_system(ft: FILETIME) -> Option<std::time::SystemTime> {
    const EPOCH_DIFF_SECS: u64 = 11_644_473_600;
    const TICKS_PER_SEC: u64 = 10_000_000;
    let ticks = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
    let secs = ticks / TICKS_PER_SEC;
    let nanos = (ticks % TICKS_PER_SEC) * 100;
    if secs < EPOCH_DIFF_SECS {
        return None;
    }
    Some(std::time::UNIX_EPOCH + std::time::Duration::new(secs - EPOCH_DIFF_SECS, nanos as u32))
}

/// tasklist prints "INFO: ..." instead of a row when no process matches.
fn pid_alive(pid: u32) -> bool {
    Command::new(TASKLIST)
        .args([
            TASKLIST_ARG_FILTER,
            &format!("PID eq {pid}"),
            TASKLIST_ARG_NOHEADER,
        ])
        .output()
        .map(|out| {
            let text = String::from_utf8_lossy(&out.stdout);
            text.contains(&pid.to_string())
        })
        .unwrap_or(false)
}
