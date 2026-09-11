//! Locate the newest codex rollout session file and pull the latest
//! `rate_limits` object out of it.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::snapshot::RateLimits;

pub const SESSIONS_DIR: &str = "sessions";
pub const ROLLOUT_PREFIX: &str = "rollout-";
pub const ROLLOUT_SUFFIX: &str = ".jsonl";
pub const RATE_LIMITS_KEY: &str = "rate_limits";

#[derive(Debug, thiserror::Error)]
pub enum RolloutError {
    #[error("no rollout file under {0:?}")]
    NotFound(PathBuf),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("no {RATE_LIMITS_KEY} in {0:?}")]
    NoRateLimits(PathBuf),
}

/// Newest `rollout-*.jsonl` under `<codex_home>/sessions` (walks YYYY/MM/DD).
pub fn newest_rollout(codex_home: &Path) -> Result<PathBuf, RolloutError> {
    let sessions = codex_home.join(SESSIONS_DIR);
    let mut newest: Option<(fs::Metadata, PathBuf)> = None;
    for entry in walk(&sessions)? {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(ROLLOUT_PREFIX) || !name.ends_with(ROLLOUT_SUFFIX) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let newer = newest
            .as_ref()
            .is_none_or(|(best, _)| meta.modified().ok() > best.modified().ok());
        if newer {
            newest = Some((meta, entry.path()));
        }
    }
    newest.map(|(_, p)| p).ok_or(RolloutError::NotFound(sessions))
}

fn walk(dir: &Path) -> Result<Vec<fs::DirEntry>, RolloutError> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err.into()),
        };
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                stack.push(entry.path());
            } else {
                out.push(entry);
            }
        }
    }
    Ok(out)
}

/// Scan the file line by line and return the last `rate_limits` object found.
pub fn latest_rate_limits(path: &Path) -> Result<RateLimits, RolloutError> {
    let content = fs::read_to_string(path)?;
    let mut found = None;
    for line in content.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(limits) = find_key(&value, RATE_LIMITS_KEY)
            .and_then(|limits| serde_json::from_value::<RateLimits>(limits.clone()).ok())
        {
            found = Some(limits);
        }
    }
    found.ok_or_else(|| RolloutError::NoRateLimits(path.to_path_buf()))
}

fn find_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => map
            .get(key)
            .or_else(|| map.values().find_map(|v| find_key(v, key))),
        Value::Array(items) => items.iter().find_map(|v| find_key(v, key)),
        _ => None,
    }
}
