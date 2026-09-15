//! Chat turn value objects exchanged over the ChatHandling port.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::domain::{SearchMode, ToolUse};

/// Cooperative cancellation flag shared between the HTTP layer and the
/// blocking generation loop.
#[derive(Clone, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Driving-adapter request: one chat turn.
pub struct ChatCmd {
    pub message: String,
    pub mode: SearchMode,
    pub max_tokens: usize,
    pub tokenizer: susutaku_mlx::tok::TokKind,
    /// Emit a reasoning block (gemma `<|think|>` system turn / qwen native
    /// thinking). Default off.
    pub think: bool,
    /// Caller bearer token; when present, task board tools are offered and
    /// the token is passed to them for role checks. None disables board tools.
    pub board_token: Option<String>,
    /// Chat agent name; when set, the agent's configured tool allow-list
    /// (`agent_settings.allowed_tools`) restricts which tools it may use.
    pub agent: Option<String>,
    /// Optional attached image (data URL); sent on the first model call only.
    pub image: Option<String>,
    /// Memory scope: chat thread id. Memories are recalled/stored per thread;
    /// None disables memory entirely (no shared scope).
    pub thread_id: Option<String>,
    /// Target task card: artifacts produced this turn (written files,
    /// shell-created images/text) are attached to it as card resources.
    pub card_id: Option<i64>,
}

/// Driving-adapter response: one completed chat turn.
pub struct ChatOutcome {
    pub model: Option<String>,
    pub text: String,
    pub searched: bool,
    /// Tool calls made this turn, in order (denied calls included).
    pub tools: Vec<ToolUse>,
    /// Recalled memory lines prepended to the context; empty = none used.
    pub memories: Vec<String>,
    pub stats: susutaku_mlx::stats::GenStats,
}
