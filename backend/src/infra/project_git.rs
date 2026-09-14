//! Resolves the git repo bound to a chat thread's project: thread → project →
//! repo setting. Settings are re-read from disk per lookup so binding changes
//! apply without a restart; secrets stay host-side.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::domain::BoundRepo;
use crate::infra::settings::git;
use crate::infra::zai::settings::SettingsState;
use crate::port::outbound::ProjectGit;

pub struct SettingsProjectGit {
    store: Arc<kanban_rs::Store>,
}

impl SettingsProjectGit {
    pub fn new(store: Arc<kanban_rs::Store>) -> Self {
        Self { store }
    }

    async fn bound_repo_inner(&self, thread_id: &str) -> Option<BoundRepo> {
        let id: i64 = thread_id.trim().parse().ok()?;
        let thread = self.store.chat_thread(id).await.ok()??;
        let project_id = thread.project_id?;
        let repo = SettingsState::load().git_repo(project_id)?;
        Some(BoundRepo {
            project_id: repo.project_id,
            display_url: git::redact_url(&repo.url),
            url: repo.url,
            secret: repo.secret,
        })
    }
}

impl ProjectGit for SettingsProjectGit {
    fn bound_repo<'a>(
        &'a self,
        thread_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<BoundRepo>> + Send + 'a>> {
        Box::pin(self.bound_repo_inner(thread_id))
    }
}
