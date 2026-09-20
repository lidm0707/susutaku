//! Tool-call policy corpus + shared katgpt tokenizer.
//!
//! The corpus is known-good tool-call lines plus the English verb corpus:
//! the BPE trained over it becomes the token gate for everything entering
//! latenspace — exact token counts for the transcript pruner, and a
//! vocabulary the policy check can diff against. Engine-free: katgpt
//! tokenizer only, no MLX / local model.

use crate::corpus;
use crate::engine::VOCAB_SIZE;
use katgpt_tokenizer::{BpeTokenizer, BpeTokenizerImpl, BpeTrainer};
use std::sync::OnceLock;

/// Known-good tool-call lines: the shapes the work loop accepts, one per
/// line (chain-gram / BPE treat each line as an independent sequence).
pub const TOOL_CORPUS: &[&str] = &[
    "TOOL: SHELL cargo check",
    "TOOL: SHELL cargo test",
    "TOOL: SHELL git status",
    "TOOL: GIT STATUS",
    "TOOL: GIT DIFF",
    "TOOL: GIT COMMIT run produced by agent",
    "TOOL: GIT PUSH task branch",
    "TOOL: GIT PR run title",
    "TOOL: AGENT_RUN ls -la",
    "TOOL: BOARD_LIST",
    "TOOL: CARD_FIND query text",
    "TOOL: SEARCH query text",
    "TOOL: FETCH url",
    "TOOL: MATH 1 + 2",
    "{\"tool\": \"shell\", \"cmd\": \"cargo check\"}",
    "{\"tool\": \"git\", \"op\": \"STATUS\"}",
    "{\"tool\": \"git\", \"op\": \"COMMIT\", \"message\": \"agent run\"}",
    "{\"tool\": \"git\", \"op\": \"PUSH\", \"branch\": \"task/branch\"}",
    "{\"tool\": \"coding\", \"path\": \"src/main.rs\", \"code\": \"fn main() {}\"}",
    "{\"tool\": \"done\", \"message\": \"feature complete, tests pass\"}",
];

/// Training text: verb corpus + tool-call lines.
pub fn policy_text() -> String {
    let mut text = corpus::text();
    text.push_str(&TOOL_CORPUS.join("\n"));
    text
}

/// Shared BPE trained once over the policy corpus.
pub fn shared_tokenizer() -> &'static BpeTokenizer {
    static TOK: OnceLock<BpeTokenizer> = OnceLock::new();
    TOK.get_or_init(|| BpeTrainer::train(&policy_text(), VOCAB_SIZE))
}

/// Encode `text` with the policy-trained BPE.
pub fn encode(text: &str) -> Vec<usize> {
    BpeTokenizerImpl::encode(shared_tokenizer(), text)
}

/// Decode `ids` back to text.
pub fn decode(ids: &[usize]) -> String {
    BpeTokenizerImpl::decode(shared_tokenizer(), ids)
}

/// Exact katgpt token count — the counter latenspace's `ToolPruner`
/// budget is measured in (replaces the chars-per-4 estimate).
pub fn token_count(text: &str) -> usize {
    encode(text).len()
}
