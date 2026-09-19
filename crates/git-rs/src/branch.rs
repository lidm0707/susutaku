//! Branch helpers: create + check out a task branch from HEAD.

use crate::error::GitError;
use crate::repo::GitRepo;

impl GitRepo {
    /// Creates `name` at HEAD (kept as-is when it already exists) and makes
    /// it the current branch. Fails when the repo has no commits.
    pub fn create_checkout_branch(&self, name: &str) -> Result<(), GitError> {
        if self
            .raw()
            .find_branch(name, git2::BranchType::Local)
            .is_err()
        {
            let commit = self.raw().head()?.peel_to_commit()?;
            self.raw().branch(name, &commit, false)?;
        }
        self.checkout_branch(name)
    }

    /// Creates `name` at HEAD when missing, otherwise MOVES it to HEAD, then
    /// checks it out. Publish uses this so the task branch always contains
    /// the run's commits, even when the agent switched branches mid-run.
    pub fn move_branch_to_head(&self, name: &str) -> Result<(), GitError> {
        let commit = self.raw().head()?.peel_to_commit()?;
        match self.raw().find_branch(name, git2::BranchType::Local) {
            Ok(mut branch) => {
                branch
                    .get_mut()
                    .set_target(commit.id(), "reconcile task branch")?;
            }
            Err(_) => {
                self.raw().branch(name, &commit, false)?;
            }
        }
        self.checkout_branch(name)
    }

    /// Checks out the local branch `name` (work tree + HEAD ref).
    pub fn checkout_branch(&self, name: &str) -> Result<(), GitError> {
        let commit = self
            .raw()
            .find_branch(name, git2::BranchType::Local)?
            .get()
            .peel_to_commit()?;
        self.raw().checkout_tree(commit.tree()?.as_object(), None)?;
        self.raw().set_head(&format!("refs/heads/{name}"))?;
        Ok(())
    }

    /// Pushes `branch` to `url` (all local branches refspec), `token` used
    /// for https basic auth when present.
    pub fn push_branch(
        &self,
        url: &str,
        branch: &str,
        token: Option<&str>,
    ) -> Result<(), GitError> {
        let mut callbacks = git2::RemoteCallbacks::new();
        if let Some(token) = token {
            let token = token.to_owned();
            callbacks.credentials(move |_, _, _| {
                git2::Cred::userpass_plaintext(crate::clone::TOKEN_USER, &token)
            });
        }
        let mut push = git2::PushOptions::new();
        push.remote_callbacks(callbacks);
        let mut remote = self.raw().remote_anonymous(url)?;
        let spec = format!("refs/heads/{branch}:refs/heads/{branch}");
        remote.push(&[spec.as_str()], Some(&mut push))?;
        Ok(())
    }
}
