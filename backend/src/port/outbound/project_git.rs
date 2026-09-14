//! Port: resolves the git repo bound to a chat thread's project, so the model
//! can clone without knowing the URL (credentials never reach the prompt).

use crate::domain::BoundRepo;
use std::future::Future;
use std::pin::Pin;

pub trait ProjectGit: Send + Sync + 'static {
    fn bound_repo<'a>(
        &'a self,
        thread_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<BoundRepo>> + Send + 'a>>;
}
