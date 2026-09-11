//! Repository handle: open an existing work tree, answer state questions.

use std::path::{Path, PathBuf};

use crate::error::GitError;

pub struct GitRepo {
    root: PathBuf,
    inner: git2::Repository,
}

impl GitRepo {
    pub fn open(work_tree: &Path) -> Result<Self, GitError> {
        let inner = git2::Repository::discover(work_tree)?;
        let root = inner
            .workdir()
            .unwrap_or_else(|| inner.path())
            .to_path_buf();
        Ok(Self { root, inner })
    }

    pub fn init(work_tree: &Path) -> Result<Self, GitError> {
        let inner = git2::Repository::init(work_tree)?;
        let root = work_tree.to_path_buf();
        Ok(Self { root, inner })
    }

    pub fn open_or_init(work_tree: &Path) -> Result<Self, GitError> {
        match Self::open(work_tree) {
            Ok(repo) => Ok(repo),
            Err(GitError::Git(_)) => Self::init(work_tree),
            Err(e) => Err(e),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn raw(&self) -> &git2::Repository {
        &self.inner
    }

    /// `true` when HEAD can be resolved (repo has at least one commit).
    pub fn has_commits(&self) -> bool {
        self.inner.head().is_ok()
    }

    pub fn head_oid(&self) -> Result<git2::Oid, GitError> {
        let head = self.inner.head()?;
        Ok(head.peel_to_commit()?.id())
    }

    pub(crate) fn empty_tree_oid(&self) -> Result<git2::Oid, GitError> {
        let tb = self.inner.treebuilder(None)?;
        Ok(tb.write()?)
    }

    pub fn is_dirty(&self) -> Result<bool, GitError> {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true);
        for entry in self.inner.statuses(Some(&mut opts))?.iter() {
            if entry.status().intersects(
                git2::Status::INDEX_NEW
                    | git2::Status::INDEX_MODIFIED
                    | git2::Status::INDEX_DELETED
                    | git2::Status::WT_MODIFIED
                    | git2::Status::WT_DELETED
                    | git2::Status::WT_NEW,
            ) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
