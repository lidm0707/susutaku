//! Sandbox directories, ownership metadata, and stale-state sweeping.
//! Layout: each sandbox root holds `workspace/` (the only writable bind)
//! plus a metadata file naming its owner process.

use std::fs;
use std::io::Error;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::WORKSPACE_MOUNT;

pub const SANDBOX_PREFIX: &str = "susutaku-agent-sandbox-";
pub const STATE_FILE: &str = "agent-sandbox-state.json";

const METADATA_FILE: &str = "sandbox-metadata.json";
const SANDBOX_SUBDIR: &str = "sandbox";
const SNAPSHOTS_SUBDIR: &str = "snapshots";
const URANDOM: &str = "/dev/urandom";
const ID_BYTES: usize = 16;
const STATE_DIR_PREFIX: &str = "susutaku-agent-state-";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SandboxMetadata {
    pub(crate) sandbox_id: String,
    pub(crate) owner_pid: u32,
    pub(crate) created_at_unix: u64,
}

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
                .map(|m| pid_alive(m.owner_pid))
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

pub(crate) fn purge_stale_dirs() {
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
            let _ = fs::remove_dir_all(entry.path());
            continue;
        };
        if meta.owner_pid == own_pid {
            continue;
        }
        if !pid_alive(meta.owner_pid) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// Delete the sandbox owned by `pid`. Refuses the caller's own sandbox and
/// any dir whose owner is still alive.
pub fn purge_dir(pid: u32) -> Result<bool, String> {
    if pid == std::process::id() {
        return Err("cannot purge the running backend's own sandbox".into());
    }
    let Some(entry) = list_dirs().into_iter().find(|d| d.pid == pid) else {
        return Ok(false);
    };
    if pid_alive(pid) {
        return Err(format!("sandbox pid {pid} is still alive; refusing purge"));
    }
    fs::remove_dir_all(&entry.path)
        .map(|_| true)
        .map_err(|e| e.to_string())
}

/// State dir is per backend instance: two backend processes can never
/// restore each other's transcripts.
pub(crate) fn instance_state_dir() -> Result<PathBuf, Error> {
    let dir = std::env::temp_dir().join(format!("{STATE_DIR_PREFIX}{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub(crate) fn purge_stale_state() {
    let own_pid = std::process::id();
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.strip_prefix(STATE_DIR_PREFIX))
            .and_then(|p| p.parse::<u32>().ok())
        else {
            continue;
        };
        if pid != own_pid && !pid_alive(pid) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

pub(crate) fn sandbox_root(sandbox_id: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{SANDBOX_PREFIX}{sandbox_id}"))
}

/// Own-pid state files whose sandbox root is already gone (dropped sandbox):
/// the state file outlives the process by design, but without a root there
/// is nothing left to restore.
pub(crate) fn purge_orphan_state_files() {
    let Ok(dir) = instance_state_dir() else {
        return;
    };
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        if !sandbox_root(id).exists() {
            let _ = fs::remove_file(entry.path());
        }
    }
}

pub(crate) fn create_workspace_layout(root: &Path) -> Result<(), Error> {
    fs::create_dir_all(root.join(WORKSPACE_MOUNT))
}

pub(crate) fn write_metadata(root: &Path, sandbox_id: &str) -> Result<(), Error> {
    let meta = SandboxMetadata {
        sandbox_id: sandbox_id.to_owned(),
        owner_pid: std::process::id(),
        created_at_unix: unix_now(),
    };
    let raw = serde_json::to_string(&meta).map_err(|e| Error::other(e.to_string()))?;
    fs::write(root.join(METADATA_FILE), raw)
}

pub(crate) fn state_file_path(sandbox_id: &str) -> Result<PathBuf, Error> {
    Ok(instance_state_dir()?.join(format!("{sandbox_id}.json")))
}

pub(crate) fn random_hex_id() -> Result<String, Error> {
    let mut bytes = [0u8; ID_BYTES];
    fs::File::open(URANDOM)?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub(crate) fn snapshots_subdir() -> &'static str {
    SNAPSHOTS_SUBDIR
}

pub(crate) fn sandbox_subdir() -> &'static str {
    SANDBOX_SUBDIR
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(unix)]
pub(crate) fn pid_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(not(unix))]
pub(crate) fn pid_alive(_pid: u32) -> bool {
    true // never delete on doubt
}

/// Host-side resolution of the stored relative cwd; the workspace bind is
/// the real boundary, this keeps stored state canonical.
pub(crate) fn resolve_cwd(root: &Path, rel: &str) -> PathBuf {
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

pub fn state_path() -> PathBuf {
    instance_state_dir().unwrap_or_else(|_| std::env::temp_dir().join(STATE_DIR_PREFIX))
}

/// Transcript of the sandbox at `sandbox_path`, read from its persisted
/// state file (`susutaku-agent-state-<owner-pid>/<sandbox-id>.json`).
/// Works for dead owners too — the state file outlives the process.
pub fn load_transcript(
    sandbox_path: &Path,
) -> Result<Vec<crate::sandbox_abstract_layer::HistoryEntry>, String> {
    let meta = read_metadata(sandbox_path)
        .ok_or_else(|| "no sandbox metadata at the given path".to_string())?;
    let file = std::env::temp_dir()
        .join(format!("{STATE_DIR_PREFIX}{}", meta.owner_pid))
        .join(format!("{}.json", meta.sandbox_id));
    // The state file is written on the first transcript entry; a sandbox
    // that has not run anything yet legitimately has none.
    if !file.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&file)
        .map_err(|e| format!("sandbox state file {}: {e}", file.display()))?;
    let state: crate::sandbox_abstract_layer::SandboxState =
        serde_json::from_str(&raw).map_err(|e| format!("corrupt sandbox state: {e}"))?;
    Ok(state.history)
}
