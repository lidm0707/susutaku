//! Linked git worktrees: one agent slot = one worktree of a shared cache
//! repo. Adding a worktree is O(1) — no object copy — and each slot still
//! gets its own branch + checkout. The admin layout matches what
//! `git worktree add` writes, so list/prune/repair interoperate. Host-side
//! only.

use std::fs;
use std::path::Path;

use crate::error::GitError;
use crate::repo::GitRepo;

impl GitRepo {
    /// Creates a linked worktree at `path` checked out on a fresh local
    /// branch at HEAD (a stale branch from a reclaimed slot is recreated at
    /// the current HEAD), or detached at HEAD with `None`. Requires HEAD to
    /// resolve.
    pub fn worktree_add(&self, path: &Path, branch: Option<&str>) -> Result<(), GitError> {
        let raw = self.raw();
        let head = raw.head()?.peel_to_commit()?;
        if let Some(branch) = branch {
            // A branch left over from a reclaimed slot points at an old HEAD;
            // recreate it so the task always starts from the fresh base.
            if let Ok(mut stale) = raw.find_branch(branch, git2::BranchType::Local) {
                stale.delete()?;
            }
            raw.branch(branch, &head, false)?;
        }
        let name = worktree_name(path);
        let git_dir = raw
            .path()
            .canonicalize()
            .unwrap_or_else(|_| raw.path().to_path_buf());
        let admin = git_dir.join("worktrees").join(&name);
        fs::create_dir_all(&admin).map_err(GitError::Io)?;
        fs::create_dir_all(path).map_err(GitError::Io)?;
        let wt_git = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .join(".git");

        // The layout git itself writes: admin/{commondir,gitdir,HEAD} plus
        // the worktree's .git file pointing back at the admin dir.
        fs::write(admin.join("commondir"), b"../..\n").map_err(GitError::Io)?;
        fs::write(admin.join("gitdir"), format!("{}\n", wt_git.display())).map_err(GitError::Io)?;
        match branch {
            Some(b) => fs::write(admin.join("HEAD"), format!("ref: refs/heads/{b}\n"))
                .map_err(GitError::Io)?,
            None => {
                fs::write(admin.join("HEAD"), format!("{}\n", head.id())).map_err(GitError::Io)?
            }
        };
        fs::write(wt_git, format!("gitdir: {}\n", admin.display())).map_err(GitError::Io)?;

        // Populate the work tree + the per-slot index via a checkout of the
        // branch HEAD in the newly linked repo.
        let wt_repo = git2::Repository::open(path)?;
        let mut opts = git2::build::CheckoutBuilder::new();
        opts.force();
        wt_repo.checkout_head(Some(&mut opts))?;
        Ok(())
    }

    /// Removes the linked worktree at `path`. Without `force`, a dirty
    /// working tree is refused — the caller decides what to do with the
    /// uncommitted work before forcing.
    pub fn worktree_remove(&self, path: &Path, force: bool) -> Result<(), GitError> {
        let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        for name in self.raw().worktrees()?.iter().flatten() {
            let Some(wt) = self.raw().find_worktree(name).ok() else {
                continue;
            };
            let wt_path = wt.path().to_path_buf();
            if wt_path != target {
                continue;
            }
            if !force
                && let Ok(wt_repo) = GitRepo::open(&wt_path)
                && wt_repo.is_dirty()?
            {
                return Err(GitError::Git(format!(
                    "worktree {} is dirty; refusing non-forced removal",
                    wt_path.display()
                )));
            }
            let mut opts = git2::WorktreePruneOptions::new();
            opts.valid(true).working_tree(true).locked(true);
            wt.prune(Some(&mut opts))?;
            return Ok(());
        }
        Err(GitError::Git(format!(
            "{} is not a linked worktree",
            path.display()
        )))
    }

    /// Drops admin metadata for worktrees whose directory is gone (crashed
    /// runs, reclaimed slots). Working trees still on disk are left alone.
    pub fn worktree_prune(&self) -> Result<(), GitError> {
        for name in self.raw().worktrees()?.iter().flatten() {
            if let Ok(wt) = self.raw().find_worktree(name) {
                let _ = wt.prune(None);
            }
        }
        Ok(())
    }

    /// Names of linked worktrees (debug/admin surface).
    pub fn worktree_names(&self) -> Result<Vec<String>, GitError> {
        Ok(self
            .raw()
            .worktrees()?
            .iter()
            .flatten()
            .map(str::to_owned)
            .collect())
    }
}

/// Worktree admin name: unique per full path (every slot's path ends in
/// `sandbox/workspace`, so the bare file name would collide across slots
/// and clobber the shared admin dir). A short hash of the canonical path
/// keeps the single-segment name readable and collision-free.
fn worktree_name(path: &Path) -> String {
    let base = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "slot".to_owned());
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&canonical, &mut hasher);
    format!("{base}-{:08x}", hasher.finish() as u32)
}
