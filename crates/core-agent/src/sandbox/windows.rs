use std::fs;
use std::io::Error;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

pub const SHELL: &str = "powershell";
pub const RUN_FLAG: &str = "-Command";
pub const MAX_OUTPUT_BYTES: usize = 1 << 20;
pub const MAX_HISTORY: usize = 128;
pub const SANDBOX_PREFIX: &str = "susutaku-agent-sandbox-";
pub const STATE_FILE: &str = "agent-sandbox-state.json";
pub const TASKLIST: &str = "tasklist";
pub const TASKLIST_ARG_FILTER: &str = "/FI";
pub const TASKLIST_ARG_NOHEADER: &str = "/NH";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    User,
    Agent,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SandboxState {
    pub cwd: String,
    pub history: Vec<HistoryEntry>,
}

pub fn run(cmd: &str) -> Result<String, Error> {
    let out = Command::new(SHELL).arg(RUN_FLAG).arg(cmd).output()?;
    if !out.status.success() {
        return Err(std::io::Error::other(
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ));
    }
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.truncate(MAX_OUTPUT_BYTES);
    Ok(text)
}

pub struct Sandbox {
    root: RwLock<PathBuf>,
    state_path: PathBuf,
    state: RwLock<SandboxState>,
}

impl Sandbox {
    pub fn new() -> Result<Self, Error> {
        let root = std::env::temp_dir().join(format!("{SANDBOX_PREFIX}{}", std::process::id()));
        fs::create_dir_all(&root)?;
        let state_path = state_path();
        let state = RwLock::new(SandboxState {
            cwd: ".".into(),
            history: Vec::new(),
        });
        Ok(Self {
            root: RwLock::new(root),
            state_path,
            state,
        })
    }

    /// Reload state from the JSON file and recreate the sandbox dir.
    pub fn restore() -> Result<Self, Error> {
        let state_path = state_path();
        let state: SandboxState = match fs::read_to_string(&state_path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => SandboxState::default(),
        };
        let root = std::env::temp_dir().join(format!("{SANDBOX_PREFIX}{}", std::process::id()));
        fs::create_dir_all(&root)?;
        Ok(Self {
            root: RwLock::new(root),
            state_path,
            state: RwLock::new(state),
        })
    }

    /// Remove leftover state of dead processes (call at backend startup).
    pub fn purge_stale() {
        let _ = fs::remove_file(state_path());
        purge_stale_dirs();
    }

    /// Workspace root the sandboxed commands run in.
    pub fn root(&self) -> PathBuf {
        self.root
            .read()
            .map(|r| r.clone())
            .unwrap_or_else(|_| std::env::temp_dir())
    }

    /// Delete sandbox dir and saved state (full teardown).
    pub fn purge(&self) {
        if let Ok(root) = self.root.read() {
            let _ = fs::remove_dir_all(&*root);
        }
        let _ = fs::remove_file(&self.state_path);
    }

    pub fn run(&self, cmd: &str) -> Result<String, Error> {
        let root = self.root.read().map_err(|e| Error::other(e.to_string()))?;
        let cwd_rel = self
            .state
            .read()
            .map(|s| s.cwd.clone())
            .unwrap_or_else(|_| ".".into());
        let cwd = resolve_cwd(&root, &cwd_rel);
        let out = Command::new(SHELL)
            .arg(RUN_FLAG)
            .arg(cmd)
            .current_dir(&cwd)
            .output()?;
        if !out.status.success() {
            return Err(std::io::Error::other(
                String::from_utf8_lossy(&out.stderr).into_owned(),
            ));
        }
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.truncate(MAX_OUTPUT_BYTES);
        drop(root);
        self.record(HistoryEntry {
            role: Role::Tool,
            content: format!("$ {cmd}\n{text}"),
        });
        Ok(text)
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
        if let Ok(mut s) = self.state.write() {
            s.cwd = rel.into();
        }
        self.save()
    }

    fn record(&self, entry: HistoryEntry) {
        if let Ok(mut s) = self.state.write() {
            s.history.push(entry);
            let excess = s.history.len().saturating_sub(MAX_HISTORY);
            s.history.drain(..excess);
        }
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

    fn clear(&self) {
        if let Ok(root) = self.root.read() {
            let _ = fs::remove_dir_all(&*root);
        }
    }
}

pub fn state_path() -> PathBuf {
    std::env::temp_dir().join(STATE_FILE)
}

/// Delete sandbox dirs whose owning backend process is gone.
fn purge_stale_dirs() {
    let own_pid = std::process::id();
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Some(pid) = name
            .strip_prefix(SANDBOX_PREFIX)
            .and_then(|raw| raw.parse::<u32>().ok())
        else {
            continue;
        };
        if pid != own_pid && !pid_alive(pid) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
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

#[derive(Debug, Clone, serde::Serialize)]
pub struct SandboxDir {
    pub pid: u32,
    pub alive: bool,
    pub path: PathBuf,
}

/// All sandbox dirs in TMPDIR with their owning-process liveness.
pub fn list_dirs() -> Vec<SandboxDir> {
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return Vec::new();
    };
    let mut dirs: Vec<SandboxDir> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let pid = name.strip_prefix(SANDBOX_PREFIX)?.parse::<u32>().ok()?;
            Some(SandboxDir {
                pid,
                alive: pid_alive(pid),
                path: entry.path(),
            })
        })
        .collect();
    dirs.sort_by_key(|d| d.pid);
    dirs
}

/// Delete the sandbox dir of `pid`. Refuses the caller's own (live) dir.
pub fn purge_dir(pid: u32) -> Result<bool, String> {
    if pid == std::process::id() {
        return Err("cannot purge the running backend's own sandbox".into());
    }
    let path = std::env::temp_dir().join(format!("{SANDBOX_PREFIX}{pid}"));
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_dir_all(&path)
        .map(|_| true)
        .map_err(|e| e.to_string())
}

fn resolve_cwd(root: &Path, rel: &str) -> PathBuf {
    let candidate = root.join(rel.trim_start_matches('/'));
    if candidate.is_dir() {
        candidate
    } else {
        root.to_path_buf()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        self.clear();
    }
}
