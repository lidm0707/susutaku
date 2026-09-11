//! Periodic task: read the newest rollout rate_limits and store a snapshot.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::time::MissedTickBehavior;

use crate::rollout;
use crate::store::Store;

pub const POLL_SECS: u64 = 300;
/// Cheap prompt sent through `codex exec` when fresh-snapshot mode is on.
pub const FRESH_PROMPT: &str = "hi";
pub const FRESH_MODEL: Option<&str> = None;
pub const FRESH_WORKSPACE: &str = "/tmp";
pub const CODEX_EXEC_BIN: &str = "codex";

/// Spawn the polling loop. Errors are logged, never fatal.
/// With `fresh` set, runs a cheap `codex exec` first so codex writes a
/// current `rate_limits` snapshot into a rollout file (costs a little quota).
pub fn spawn(store: Arc<Store>, codex_home: PathBuf, fresh: bool) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(POLL_SECS));
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            poll_once(&store, &codex_home, fresh).await;
        }
    })
}

/// One scheduler pass: optionally trigger a fresh snapshot, then store the
/// newest rollout's rate_limits.
pub async fn poll_once(store: &Store, codex_home: &Path, fresh: bool) {
    if fresh
        && let Err(err) = tokio::task::spawn_blocking({
            let codex_home = codex_home.to_path_buf();
            move || fresh_snapshot(&codex_home)
        })
        .await
        .expect("fresh snapshot join")
    {
        tracing::warn!("codex usage fresh trigger: {err}");
    }
    match rollout::newest_rollout(codex_home).and_then(|path| rollout::latest_rate_limits(&path)) {
        Ok(limits) => {
            if let Err(err) = store.insert(&limits).await {
                tracing::warn!("codex usage insert: {err}");
            }
        }
        Err(err) => tracing::debug!("codex usage read: {err}"),
    }
}

/// Run `codex exec` with a minimal prompt and drain it so a rollout file with
/// a fresh `rate_limits` snapshot is produced. Blocking; call from a worker.
fn fresh_snapshot(codex_home: &Path) -> Result<(), String> {
    let mut child = codex_cli::exec::exec_json(
        FRESH_PROMPT,
        FRESH_MODEL,
        codex_home,
        Path::new(FRESH_WORKSPACE),
    )
    .map_err(|e| e.to_string())?;
    child
        .wait()
        .map_err(|e| e.to_string())?
        .success()
        .then_some(())
        .ok_or_else(|| format!("{CODEX_EXEC_BIN} exited non-zero"))
}
