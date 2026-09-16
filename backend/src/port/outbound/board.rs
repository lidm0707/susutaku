//! Outbound port: task board operations callable from the chat tool loop.
//! Implementations enforce authentication/authorization (bearer token, editor
//! role).

use async_trait::async_trait;

use crate::domain::{BoardRequest, BoardResult};

#[async_trait]
pub trait BoardOps: Send + Sync {
    async fn exec(&self, req: BoardRequest) -> BoardResult;

    /// Compact context snapshot of one task: state + recent comments, for
    /// grounding a chat turn on that card. None when the card does not exist.
    async fn card_context(&self, card_id: i64) -> Option<String>;
}
