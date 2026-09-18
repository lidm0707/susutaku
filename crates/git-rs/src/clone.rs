//! Clone a remote into a fresh work tree. The access token is supplied via a
//! credential callback so it never lands in the remote url or `.git/config`.

use std::path::Path;

use git2::{Cred, FetchOptions, RemoteCallbacks};

use crate::error::GitError;
use crate::repo::GitRepo;

/// Username expected by GitHub-style token auth over https.
pub const TOKEN_USER: &str = "x-access-token";

impl GitRepo {
    /// Clones `url` into `into` (must not exist or be empty). `token` is used
    /// for https basic auth when present; the stored remote keeps no secret.
    pub fn clone_into(url: &str, into: &Path, token: Option<&str>) -> Result<Self, GitError> {
        let mut callbacks = RemoteCallbacks::new();
        if let Some(token) = token {
            let token = token.to_owned();
            callbacks.credentials(move |_, _, _| Cred::userpass_plaintext(TOKEN_USER, &token));
        }
        let mut fetch = FetchOptions::new();
        fetch.remote_callbacks(callbacks);
        let mut builder = git2::build::RepoBuilder::new();
        builder.fetch_options(fetch);
        let raw = builder.clone(url, into)?;
        let root = raw.workdir().unwrap_or_else(|| raw.path()).to_path_buf();
        Ok(Self::from_parts(root, raw))
    }

    /// Force-fetches all remote branches into this repo (`+refs/heads/*`).
    /// Used to refresh a bare cache mirror before spawning worktrees from
    /// it; the cache itself is never committed to, so force-updating local
    /// heads is safe. The token never lands in the stored config.
    pub fn fetch(&self, url: &str, token: Option<&str>) -> Result<(), GitError> {
        const REFSPEC: &str = "+refs/heads/*:refs/heads/*";
        let mut callbacks = RemoteCallbacks::new();
        if let Some(token) = token {
            let token = token.to_owned();
            callbacks.credentials(move |_, _, _| Cred::userpass_plaintext(TOKEN_USER, &token));
        }
        let mut fetch = FetchOptions::new();
        fetch.remote_callbacks(callbacks);
        let mut remote = self.raw().remote_anonymous(url)?;
        remote.fetch(&[REFSPEC], Some(&mut fetch), None)?;
        Ok(())
    }

    /// Points the bare cache's HEAD at `branch` so new worktrees resolve a
    /// base commit even when the remote's default branch is not `main`.
    pub fn set_head_to_branch(&self, branch: &str) -> Result<(), GitError> {
        self.raw().set_head(&format!("refs/heads/{branch}"))?;
        Ok(())
    }
}
