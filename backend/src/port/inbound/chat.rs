//! Chat use case port.

use crate::domain::{ChatCmd, ChatOutcome};

pub trait ChatHandling: Send + Sync {
    fn execute(
        &self,
        cmd: ChatCmd,
    ) -> impl std::future::Future<Output = Result<ChatOutcome, String>> + Send;
}
