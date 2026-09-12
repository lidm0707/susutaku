//! Chat turn value objects exchanged over the ChatHandling port.

use crate::domain::SearchMode;

/// Driving-adapter request: one chat turn.
pub struct ChatCmd {
    pub message: String,
    pub mode: SearchMode,
    pub max_tokens: usize,
    pub tokenizer: susutaku_mlx::tok::TokKind,
    /// Emit a reasoning block (gemma `<|think|>` system turn / qwen native
    /// thinking). Default off.
    pub think: bool,
    /// Caller bearer token; when present, kanban board tools are offered and
    /// the token is passed to them for role checks. None disables board tools.
    pub board_token: Option<String>,
    /// Chat agent name; when set, the agent's configured tool allow-list
    /// (`agent_settings.allowed_tools`) restricts which tools it may use.
    pub agent: Option<String>,
}

/// Driving-adapter response: one completed chat turn.
pub struct ChatOutcome {
    pub model: Option<String>,
    pub text: String,
    pub searched: bool,
    pub stats: susutaku_mlx::stats::GenStats,
}
