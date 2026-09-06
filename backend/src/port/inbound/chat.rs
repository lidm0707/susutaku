//! Chat use case port.

/// Driving-adapter request: one chat turn.
pub struct ChatCmd {
    pub message: String,
    pub mode: crate::domain::SearchMode,
    pub max_tokens: usize,
    pub tokenizer: susutaku_mlx::tok::TokKind,
    /// Emit a reasoning block (gemma `<|think|>` system turn / qwen native
    /// thinking). Default off.
    pub think: bool,
}

/// Driving-adapter response: one completed chat turn.
pub struct ChatOutcome {
    pub model: Option<String>,
    pub text: String,
    pub searched: bool,
    pub stats: susutaku_mlx::engine::GenStats,
}

pub trait ChatHandling: Send + Sync {
    fn execute(
        &self,
        cmd: ChatCmd,
    ) -> impl std::future::Future<Output = Result<ChatOutcome, String>> + Send;
}
