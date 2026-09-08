//! Inference ports, equivalent to the backend outbound ports.

use susutaku_mlx::engine::GenStats;
use susutaku_mlx::tok::TokKind;

pub struct GenReply {
    pub model: String,
    pub text: String,
    pub stats: GenStats,
}

pub type ReplyRx = tokio::sync::oneshot::Receiver<Result<GenReply, String>>;

pub trait Inference: Send + Sync + 'static {
    fn submit(
        &self,
        prompt: String,
        max_tokens: usize,
        tok: TokKind,
        think: bool,
    ) -> Result<ReplyRx, String>;
}

pub trait ModelSwitch: Send + Sync + 'static {
    fn select(&self, name: &str) -> Result<(), String>;
    fn selected(&self) -> Option<String>;
}
