//! Git toolcall: basic host-side git operations against an agent's work
//! tree. Runs on the machine that owns the agent — the sandbox itself has
//! no network, so clone/status/diff must never run inside it.

use std::fs;
use std::path::Path;

use git_rs::GitRepo;
use proto_rs::GitTool;

/// Spawn-time layout inside the agent workspace that is not workspace
/// content: a fresh init repo and the snapshots dir.
const SPAWN_LAYOUT_ENTRIES: [&str; 2] = [".git", "snapshots"];

/// Executes `tool` against `workspace` — the agent's mounted workspace
/// (the dir the agent sees as `/workspace`) — returning text output.
pub fn apply(work_tree: &Path, tool: &GitTool) -> Result<String, String> {
    match tool {
        GitTool::Clone { url, token } => clone(work_tree, url, token.as_deref()),
        GitTool::Status => status(work_tree),
        GitTool::Diff => diff(work_tree),
        GitTool::Branch { .. }
        | GitTool::Commit { .. }
        | GitTool::Push { .. }
        | GitTool::PullRequest { .. } => {
            Err("branch/commit/push/pr run inside the agent's own sandbox".to_string())
        }
    }
}

/// Clones `url` into the work tree. Only a fresh workspace may be cloned
/// into: any content beyond the bare `.git` a spawn creates is refused.
fn clone(workspace: &Path, url: &str, token: Option<&str>) -> Result<String, String> {
    let stale = || {
        std::fs::read_dir(workspace)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|e| !SPAWN_LAYOUT_ENTRIES.contains(&e.file_name().to_str().unwrap_or("")))
            })
            .unwrap_or(false)
    };
    if stale() {
        return Err(format!(
            "workspace {} is not empty; clone needs a fresh workspace",
            workspace.display()
        ));
    }
    // `git clone` refuses non-empty targets — clear the spawn layout first.
    for entry in SPAWN_LAYOUT_ENTRIES {
        let _ = fs::remove_dir_all(workspace.join(entry));
    }
    let _ = fs::remove_file(workspace.join(".git"));
    GitRepo::clone_into(url, workspace, token)
        .map(|_| format!("cloned {url} into {}", workspace.display()))
        .map_err(|e| e.to_string())
}

fn status(workspace: &Path) -> Result<String, String> {
    let repo = GitRepo::open(workspace).map_err(|e| e.to_string())?;
    let head = repo
        .head_oid()
        .map(|oid| oid.to_string())
        .unwrap_or_else(|_| "no commits".to_string());
    let dirty = repo.is_dirty().map_err(|e| e.to_string())?;
    Ok(format!(
        "HEAD {head}\n{}",
        if dirty { "dirty" } else { "clean" }
    ))
}

fn diff(workspace: &Path) -> Result<String, String> {
    let repo = GitRepo::open(workspace).map_err(|e| e.to_string())?;
    let patch = repo.patch_workdir().map_err(|e| e.to_string())?;
    if patch.is_empty() {
        Ok("(no changes)".to_string())
    } else {
        Ok(patch)
    }
}
