//! Patch text: work tree vs a base ref, between refs, and whole-task capture.

use crate::error::GitError;
use crate::repo::GitRepo;
use crate::{DIFF_CONTEXT_LINES, TASK_COMMIT_PREFIX};

/// Everything a finished agent task produced: unified patch + commit oid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPatch {
    pub patch: String,
    pub commit: Option<String>,
}

impl GitRepo {
    /// Captures everything the task changed: commits pending work, then diffs
    /// `base_commit` (or the empty tree) against the new HEAD.
    pub fn task_patch(
        &self,
        base_commit: Option<&str>,
        message: &str,
    ) -> Result<TaskPatch, GitError> {
        let unborn = !self.has_commits();
        let commit = if self.is_dirty()? {
            Some(self.commit_all(&format!("{TASK_COMMIT_PREFIX}{message}"))?)
        } else {
            None
        };
        if unborn && commit.is_none() {
            return Ok(TaskPatch {
                patch: String::new(),
                commit: None,
            });
        }
        let head = self.head_oid()?;
        let patch = match base_commit {
            Some(base) if !unborn => {
                let from = git2::Oid::from_str(base)?;
                if from == head {
                    String::new()
                } else {
                    self.patch_range(from, head)?
                }
            }
            _ => self.patch_empty_to(head)?,
        };
        Ok(TaskPatch {
            patch,
            commit: commit.map(|c| c.to_string()),
        })
    }

    pub fn patch_range(&self, from: git2::Oid, to: git2::Oid) -> Result<String, GitError> {
        self.patch_between_commits(from, to)
    }

    pub fn patch_empty_to(&self, to: git2::Oid) -> Result<String, GitError> {
        let empty = self.raw().find_tree(self.empty_tree_oid()?)?;
        let to_tree = self.raw().find_commit(to)?.tree()?;
        let mut opts = diff_options();
        let diff = self
            .raw()
            .diff_tree_to_tree(Some(&empty), Some(&to_tree), Some(&mut opts))?;
        patch_text(&diff)
    }

    /// Working tree (staged + unstaged + untracked) vs HEAD.
    pub fn patch_workdir(&self) -> Result<String, GitError> {
        let head = self.head_oid()?;
        self.patch_between_tree_and_workdir(head)
    }

    /// Work-tree patch against the empty tree — the unborn-HEAD case (fresh
    /// repo, no commits yet).
    pub fn patch_workdir_empty_base(&self) -> Result<String, GitError> {
        let empty = self.raw().find_tree(self.empty_tree_oid()?)?;
        let mut opts = diff_options();
        let diff = self
            .raw()
            .diff_tree_to_workdir_with_index(Some(&empty), Some(&mut opts))?;
        patch_text(&diff)
    }

    /// Patch of the commits HEAD is ahead of the base branch
    /// (`DEFAULT_BRANCH`, local or `origin/`). Empty when HEAD sits on or
    /// behind the base branch — i.e. nothing committed but unpushed.
    pub fn patch_unpushed(&self) -> Result<String, GitError> {
        if !self.has_commits() {
            return Ok(String::new());
        }
        let head = self.head_oid()?;
        let Some(base) = self.base_branch_commit()? else {
            return Ok(String::new());
        };
        let base = base.id();
        if base == head {
            return Ok(String::new());
        }
        let merge_base = self.raw().merge_base(base, head)?;
        if merge_base == head {
            return Ok(String::new());
        }
        self.patch_between_commits(merge_base, head)
    }

    /// Base branch tip, or `None` when neither `main` nor `origin/main`
    /// exists (fresh repo, detached HEAD) — callers treat that as "no base".
    fn base_branch_commit(&self) -> Result<Option<git2::Commit<'_>>, GitError> {
        let local = self
            .raw()
            .find_branch(crate::DEFAULT_BRANCH, git2::BranchType::Local)
            .ok();
        if let Some(b) = local {
            return Ok(Some(b.get().peel_to_commit()?));
        }
        let remote_name = format!("{}{}", crate::REMOTE_ORIGIN, crate::DEFAULT_BRANCH);
        let remote = self
            .raw()
            .find_branch(&remote_name, git2::BranchType::Remote)
            .ok();
        match remote {
            Some(b) => Ok(Some(b.get().peel_to_commit()?)),
            None => Ok(None),
        }
    }
}

/// Diff kind: what a patch is computed against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchBase<'a> {
    /// Working tree (staged + unstaged + untracked) vs HEAD.
    WorkDir,
    /// One commit vs HEAD.
    Commit(&'a str),
}

impl GitRepo {
    pub fn patch(&self, base: PatchBase<'_>) -> Result<String, GitError> {
        match base {
            PatchBase::WorkDir => self.patch_workdir(),
            PatchBase::Commit(rev) => {
                let from = self.raw().revparse_single(rev)?.peel_to_commit()?.id();
                let to = self.head_oid()?;
                if from == to {
                    return Ok(String::new());
                }
                self.patch_between_commits(from, to)
            }
        }
    }

    fn patch_between_tree_and_workdir(&self, base: git2::Oid) -> Result<String, GitError> {
        let commit = self.raw().find_commit(base)?;
        let mut opts = diff_options();
        let diff = self
            .raw()
            .diff_tree_to_workdir_with_index(Some(&commit.tree()?), Some(&mut opts))?;
        patch_text(&diff)
    }

    fn patch_between_commits(&self, from: git2::Oid, to: git2::Oid) -> Result<String, GitError> {
        let from_tree = self.raw().find_commit(from)?.tree()?;
        let to_tree = self.raw().find_commit(to)?.tree()?;
        let mut opts = diff_options();
        let diff =
            self.raw()
                .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), Some(&mut opts))?;
        patch_text(&diff)
    }
}

fn diff_options() -> git2::DiffOptions {
    let mut opts = git2::DiffOptions::new();
    opts.context_lines(DIFF_CONTEXT_LINES)
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true);
    opts
}

fn patch_text(diff: &git2::Diff<'_>) -> Result<String, GitError> {
    let mut out = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let origin = line.origin();
        if matches!(origin, '+' | '-' | ' ') {
            out.push(origin);
        }
        out.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
        true
    })?;
    Ok(out)
}
