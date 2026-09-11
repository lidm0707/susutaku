//! Inference port.

use crate::domain::ReplyRx;
use susutaku_mlx::tok::TokKind;

/// Port: queues one generation on the model; the result arrives on the
/// returned receiver.
pub trait Inference: Send + Sync + 'static {
    fn submit(
        &self,
        prompt: String,
        max_tokens: usize,
        tok: TokKind,
        think: bool,
    ) -> Result<ReplyRx, String>;
}
