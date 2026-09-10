//! Inference port.

use susutaku_mlx::stats::GenStats;
use susutaku_mlx::tok::TokKind;

/// Result of one queued generation.
pub struct GenReply {
    pub model: String,
    pub text: String,
    pub stats: GenStats,
}

pub type ReplyRx = tokio::sync::oneshot::Receiver<Result<GenReply, String>>;

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
