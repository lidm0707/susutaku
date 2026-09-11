//! Outbound port: kanban board operations callable from the chat tool loop.
//! Implementations enforce authentication/authorization (bearer token, editor
//! role).

use async_trait::async_trait;

use crate::domain::{BoardRequest, BoardResult};

#[async_trait]
pub trait BoardOps: Send + Sync {
    async fn exec(&self, req: BoardRequest) -> BoardResult;
}
