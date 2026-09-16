//! Thread-scoped sandbox environments: each chat thread that runs coding
//! tools gets its own podman sandbox under `work/thread-envs/<thread_id>`,
//! so concurrent threads never share a work tree. Environments idle beyond
//! [`ENV_TTL_SECS`] are swept (map entry + directory).

use crate::infra::podman::AgentSandbox;
use crate::port::outbound::{Runner, ThreadEnvs};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

const ENVS_ROOT: &str = "work/thread-envs";
const ENV_TTL_SECS: u64 = 24 * 60 * 60;

struct EnvEntry {
    runner: Arc<AgentSandbox>,
    last_used: RwLock<u64>,
}

impl EnvEntry {
    fn touch(&self) {
        if let Ok(mut t) = self.last_used.write() {
            *t = now_secs();
        }
    }

    fn idle_secs(&self) -> u64 {
        let last = match self.last_used.read() {
            Ok(guard) => *guard,
            Err(_) => 0,
        };
        now_secs().saturating_sub(last)
    }
}

pub struct ThreadEnvManager {
    root: PathBuf,
    envs: RwLock<HashMap<String, EnvEntry>>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Dir-name-safe thread id (ids are numeric strings; strip anything else).
fn sanitize(thread_id: &str) -> String {
    thread_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

impl ThreadEnvManager {
    pub fn new() -> Self {
        let root = PathBuf::from(ENVS_ROOT);
        let _ = std::fs::create_dir_all(&root);
        Self {
            root,
            envs: RwLock::new(HashMap::new()),
        }
    }

    /// Removes environments idle beyond the TTL (map entry + directory) and
    /// orphans on disk from a previous process. Called on every resolve —
    /// cheap, no background task needed.
    fn sweep(&self, now_map: &mut HashMap<String, EnvEntry>) {
        let stale: Vec<String> = now_map
            .iter()
            .filter(|(_, e)| e.idle_secs() > ENV_TTL_SECS)
            .map(|(k, _)| k.clone())
            .collect();
        for id in stale {
            if let Some(e) = now_map.remove(&id) {
                let _ = std::fs::remove_dir_all(self.root.join(sanitize(&id)));
                drop(e);
            }
        }
        // Orphan dirs (backend restarted, map lost): fall back to dir mtime.
        if let Ok(entries) = std::fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                let fresh = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|m| m.elapsed().ok())
                    .map(|age| age.as_secs() < ENV_TTL_SECS)
                    .unwrap_or(true);
                if !fresh {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }
    }
}

impl Default for ThreadEnvManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadEnvs for ThreadEnvManager {
    fn runner_for(
        &self,
        thread_id: &str,
        _agent: Option<&str>,
    ) -> Result<Arc<dyn Runner>, String> {
        let key = sanitize(thread_id);
        if key.is_empty() {
            return Err("thread id is empty".to_string());
        }
        let mut map = self
            .envs
            .write()
            .map_err(|_| "thread env map poisoned".to_string())?;
        self.sweep(&mut map);
        if let Some(entry) = map.get(&key) {
            entry.touch();
            return Ok(entry.runner.clone());
        }
        let work_tree = self.root.join(&key);
        let runner = Arc::new(AgentSandbox::for_work_tree(&work_tree)?);
        map.insert(
            key,
            EnvEntry {
                last_used: RwLock::new(now_secs()),
                runner: runner.clone(),
            },
        );
        Ok(runner)
    }
}
