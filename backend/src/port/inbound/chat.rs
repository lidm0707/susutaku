//! Chat use case port.

use crate::domain::{ChatCmd, ChatOutcome};

pub trait ChatHandling: Send + Sync {
    fn execute(
        &self,
        cmd: ChatCmd,
    ) -> impl std::future::Future<Output = Result<ChatOutcome, String>> + Send;

    /// The inference engine behind this use case, if any; used by the
    /// pipeline runner so `model_infer` stages share the chat model.
    fn inference(&self) -> Option<std::sync::Arc<dyn crate::port::outbound::Inference>> {
        None
    }
}
