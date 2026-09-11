//! Inference result value objects.

use susutaku_mlx::stats::GenStats;

/// Result of one queued generation.
pub struct GenReply {
    pub model: String,
    pub text: String,
    pub stats: GenStats,
}

pub type ReplyRx = tokio::sync::oneshot::Receiver<Result<GenReply, String>>;
