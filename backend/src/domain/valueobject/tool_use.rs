//! Trace of one tool call made during a chat turn.

use super::ToolKind;

use crate::domain::TOOL_DENIED as TOOL_DENIED_NOTE;

pub const TOOL_SUMMARY_MAX: usize = 200;

/// What the agent did with a tool: which one, with what input, and a short
/// result summary. Shown to the user; never fed back to the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolUse {
    pub kind: ToolKind,
    pub input: String,
    pub ok: bool,
    pub summary: String,
}

impl ToolUse {
    pub fn new(kind: ToolKind, input: &str, result: &Result<String, String>) -> Self {
        let (ok, body) = match result {
            Ok(s) => (true, s.as_str()),
            Err(e) => (false, e.as_str()),
        };
        Self {
            kind,
            input: input.chars().take(TOOL_SUMMARY_MAX).collect(),
            ok,
            summary: body.chars().take(TOOL_SUMMARY_MAX).collect(),
        }
    }

    /// A call the tool allow-list refused.
    pub fn denied(kind: ToolKind, input: &str) -> Self {
        Self {
            kind,
            input: input.chars().take(TOOL_SUMMARY_MAX).collect(),
            ok: false,
            summary: TOOL_DENIED_NOTE.to_string(),
        }
    }
}
