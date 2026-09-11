//! Stage everything and create a commit on HEAD (or the initial commit).

use crate::error::GitError;
use crate::repo::GitRepo;
use crate::{AGENT_SIGNATURE_EMAIL, AGENT_SIGNATURE_NAME, STAGE_PATHSPEC};

impl GitRepo {
    pub fn commit_all(&self, message: &str) -> Result<git2::Oid, GitError> {
        self.stage_all()?;
        let tree_id = self.raw().index()?.write_tree()?;
        let tree = self.raw().find_tree(tree_id)?;
        let sig = git2::Signature::now(AGENT_SIGNATURE_NAME, AGENT_SIGNATURE_EMAIL)?;
        let parent = self.head_commit()?;
        match parent {
            Some(p) => self
                .raw()
                .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&p])
                .map_err(GitError::from),
            None => self
                .raw()
                .commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
                .map_err(GitError::from),
        }
    }

    fn stage_all(&self) -> Result<(), GitError> {
        let mut index = self.raw().index()?;
        index.add_all([STAGE_PATHSPEC], git2::IndexAddOption::DEFAULT, None)?;
        index.update_all([STAGE_PATHSPEC], None)?;
        index.write()?;
        Ok(())
    }

    fn head_commit(&self) -> Result<Option<git2::Commit<'_>>, GitError> {
        match self.raw().head() {
            Ok(head) => Ok(Some(head.peel_to_commit()?)),
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
