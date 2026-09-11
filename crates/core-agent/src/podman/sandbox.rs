//! The `Sandbox` handle: lifecycle (new/new_in/restore/purge), the
//! per-command transcript, and cwd bookkeeping. Container execution itself
//! lives in [`super::runner`].

use std::fs;
use std::io::Error;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use super::WORKSPACE_MOUNT;
use super::limits::{NetworkPolicyChoice, SandboxLimits};
use super::runner;
use super::state;
use super::state::resolve_cwd;
use crate::sandbox_abstract_layer::{Guarantee, SandboxLayer};

pub use crate::sandbox_abstract_layer::{HistoryEntry, Role, SandboxState};

const MAX_HISTORY: usize = 128;

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
    run_seq: AtomicU64,
    /// `run` holds a read guard; `purge` takes the write guard, so a
    /// workspace can never be deleted mid-command.
    run_gate: RwLock<()>,
}

/// One-shot sandbox: create, run, purge.
pub fn run(cmd: &str) -> Result<String, Error> {
    let sb = Sandbox::new()?;
    let out = sb.run(cmd)?;
    sb.purge();
    Ok(out)
}

impl Sandbox {
    pub fn new() -> Result<Self, Error> {
        state::purge_stale_dirs();
        state::purge_stale_state();
        let sandbox_id = state::random_hex_id()?;
        let root = state::sandbox_root(&sandbox_id);
        state::create_workspace_layout(&root)?;
        state::write_metadata(&root, &sandbox_id)?;
        Self::build(sandbox_id, root, None)
    }

    /// Sandbox rooted at an explicit work tree (one per agent).
    pub fn new_in(work_tree: &Path) -> Result<Self, Error> {
        let sandbox_id = state::random_hex_id()?;
        fs::create_dir_all(work_tree)?;
        let root = work_tree.join(state::sandbox_subdir());
        state::create_workspace_layout(&root)?;
        state::write_metadata(&root, &sandbox_id)?;
        Self::build(sandbox_id, root, None)
    }

    /// Reload the most recently saved state belonging to *this backend
    /// instance* and recreate its workspace.
    pub fn restore() -> Result<Self, Error> {
        let state_dir = state::instance_state_dir()?;
        let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
        for entry in fs::read_dir(&state_dir)?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
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
        let restored: SandboxState =
            serde_json::from_str(&raw).map_err(|e| Error::other(e.to_string()))?;
        let sandbox_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| Error::other("malformed state file name"))?
            .to_owned();
        let root = state::sandbox_root(&sandbox_id);
        state::create_workspace_layout(&root)?;
        Self::build(sandbox_id, root, Some((restored, path)))
    }

    fn build(
        sandbox_id: String,
        root: PathBuf,
        restored: Option<(SandboxState, PathBuf)>,
    ) -> Result<Self, Error> {
        let (state, state_file) = match restored {
            Some((s, f)) => (s, f),
            None => (
                SandboxState::default(),
                state::state_file_path(&sandbox_id)?,
            ),
        };
        Ok(Self {
            sandbox_id,
            root,
            state_file,
            state: RwLock::new(state),
            lifecycle: RwLock::new(Lifecycle::Idle),
            run_seq: AtomicU64::new(0),
            run_gate: RwLock::new(()),
        })
    }

    /// Remove leftovers of dead backend instances (call at startup).
    pub fn purge_stale() {
        state::purge_stale_dirs();
        state::purge_stale_state();
        state::purge_orphan_state_files();
    }

    /// Writable workspace (host-side path; mounted at `/workspace` inside).
    pub fn root(&self) -> PathBuf {
        self.root.join(WORKSPACE_MOUNT)
    }

    /// Copy the workspace into `root/snapshots/<label>`; dies with `purge()`.
    pub fn snapshot(&self, label: &str) -> Result<PathBuf, Error> {
        let _gate = self
            .run_gate
            .read()
            .map_err(|e| Error::other(e.to_string()))?;
        let lc = self
            .lifecycle
            .read()
            .map_err(|e| Error::other(e.to_string()))?;
        if matches!(*lc, Lifecycle::Purging | Lifecycle::Purged) {
            return Err(Error::other("sandbox already purged"));
        }
        drop(lc);
        let dst = self.root().join(state::snapshots_subdir()).join(label);
        super::copy_dir_recursive_skip(&self.root(), &dst, state::snapshots_subdir())?;
        Ok(dst)
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
        let result = self.run_inner(cmd, limits, network);
        let entry = HistoryEntry {
            role: Role::Tool,
            content: match &result {
                Ok(text) => format!("$ {cmd}\n{text}"),
                Err(e) => format!("$ {cmd}\n<error> {e}"),
            },
        };
        let recorded = self.record(entry);
        let result = match result {
            Ok(text) => recorded.map(|_| text),
            Err(e) => Err(e),
        };
        if let Ok(mut lc) = self.lifecycle.write()
            && *lc == Lifecycle::Running
        {
            *lc = Lifecycle::Idle;
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
        let seq = self.run_seq.fetch_add(1, Ordering::Relaxed);
        runner::run_container(
            &self.sandbox_id,
            seq,
            &self.root(),
            &cwd_rel,
            cmd,
            limits,
            network,
        )
    }

    pub fn push_context(&self, role: Role, content: impl Into<String>) {
        let entry = HistoryEntry {
            role,
            content: content.into(),
        };
        if let Err(e) = self.record(entry) {
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
    /// workspace and state file. Per-run containers are `--rm`; the timeout
    /// path force-removes strays, so no container teardown is needed here.
    pub fn purge(&self) {
        let Ok(_gate) = self.run_gate.write() else {
            return;
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
    }
}
