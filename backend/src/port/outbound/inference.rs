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

    /// Same as [`Self::submit`] with an optional attached image (data URL).
    /// Engines that cannot see images ignore it (default); vision-capable
    /// engines send it as a multimodal content part.
    fn submit_with_image(
        &self,
        prompt: String,
        image: Option<String>,
        max_tokens: usize,
        tok: TokKind,
        think: bool,
    ) -> Result<ReplyRx, String> {
        let _ = image;
        self.submit(prompt, max_tokens, tok, think)
    }
}
