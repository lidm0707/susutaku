//! kat tool-call gate: JSON-stuck detection for agent replies plus a
//! context-pruned tool transcript.
//!
//! [`check_json`] pulls the JSON object out of a model reply (code fences
//! allowed) and classifies the failure mode: a brace/string scan distinguishes
//! a *stuck* generation (truncated mid-object, depth never closes) from
//! plain-invalid JSON, so the agent can be sent a precise repair prompt
//! ([`repair_prompt`]) and try again. [`ToolPruner`] keeps the work-loop
//! transcript under a token budget using katgpt usage-rate eviction — cold,
//! old tool outputs are evicted first, pinned entries never.

use std::collections::HashSet;
use std::sync::OnceLock;

use katgpt_validator::PartialParser;

use crate::context::{Context, Entry, EntryKind, estimate_tokens};

pub const FENCE_JSON: &str = "```json";
pub const FENCE: &str = "```";
pub const REPAIR_PREFIX: &str = "malformed JSON tool call: nothing was run. ";
pub const REPAIR_RETRY: &str = "Repeat the complete call as one valid JSON object.";
pub const PRUNER_DEFAULT_BUDGET: usize = 4096;
pub const PARSER_BROKEN_NOTE: &str = "a closing bracket appeared with no matching opener";
pub const EXPECTED_MARK: &str = "expected";

/// Why a reply's JSON tool call could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonStuck {
    /// No JSON object anywhere in the reply.
    NoJson,
    /// Object never closes — generation stopped mid-JSON.
    Truncated { depth: usize },
    /// Complete JSON but the parser rejected it; `detail` carries the
    /// parser's own message (e.g. "expected `,` at line 1 column 20").
    Invalid { detail: String },
    /// Valid JSON but not an object (array/scalar).
    NotObject,
}

impl JsonStuck {
    pub fn as_str(self) -> &'static str {
        match self {
            JsonStuck::NoJson => "no json object",
            JsonStuck::Truncated { .. } => "truncated",
            JsonStuck::Invalid { .. } => "invalid json",
            JsonStuck::NotObject => "not an object",
        }
    }
}

// Tier 0: katgpt-validator's PartialParser — rejects the moment a closer
// appears with no opener (never false-accepts); `total_depth()` counts the
// outstanding open brackets for the truncation note.
// Tier 1: serde_json's own parse, with `CompilerFeedback`-style detail.

/// Strip an optional code fence and return the reply's innermost payload.
fn unfence(text: &str) -> &str {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix(FENCE_JSON) {
        return rest.strip_suffix(FENCE).unwrap_or(rest).trim();
    }
    if let Some(rest) = t.strip_prefix(FENCE) {
        return rest.strip_suffix(FENCE).unwrap_or(rest).trim();
    }
    t
}

/// Candidate payload slice: first `{`..last `}` (object) or first
/// `[`..last `]` (array); end-of-text end is allowed when truncated.
fn object_slice(text: &str) -> &str {
    if let Some(start) = text.find('{') {
        let end = text
            .rfind('}')
            .filter(|&i| i >= start)
            .map_or(text.len(), |i| i + 1);
        return &text[start..end];
    }
    match text.find('[') {
        Some(start) => {
            let end = text
                .rfind(']')
                .filter(|&i| i >= start)
                .map_or(text.len(), |i| i + 1);
            &text[start..end]
        }
        None => "",
    }
}

/// Extract and validate the JSON object in a model reply. `Ok(value)` means
/// the call payload is usable; `Err(stuck)` classifies what went wrong.
pub fn check_json(text: &str) -> Result<serde_json::Value, JsonStuck> {
    let body = unfence(text);
    // Tier 0 on the whole reply: a stray closer (`}` with no `{`, even in
    // surrounding prose) is broken — PartialParser rejects it immediately.
    let mut tier0 = PartialParser::new();
    if !tier0.is_valid(body) {
        return Err(JsonStuck::Invalid {
            detail: PARSER_BROKEN_NOTE.to_owned(),
        });
    }
    let slice = object_slice(body);
    if slice.is_empty() {
        return Err(JsonStuck::NoJson);
    }
    // Tier 0 on the payload: open brackets still unclosed = stuck generation.
    let mut payload = PartialParser::new();
    if !payload.is_valid(slice) || !payload.is_balanced() {
        return Err(JsonStuck::Truncated {
            depth: payload.total_depth().max(0) as usize,
        });
    }
    // Tier 1: serde parse with CompilerFeedback-style detail.
    let value: serde_json::Value = serde_json::from_str(slice).map_err(|e| JsonStuck::Invalid {
        detail: e.to_string(),
    })?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(JsonStuck::NotObject)
    }
}

/// Extract the parser's "expected …" hint from an error message, mirroring
/// katgpt-validator's `CompilerFeedback::extract_suggestion`.
fn extract_suggestion(detail: &str) -> Option<&str> {
    let start = detail.find(EXPECTED_MARK)?;
    Some(detail[start..].trim())
}

// --- policy: corpus-checked tool actions -----------------------------------

/// Known tool actions accepted in a JSON call's action field.
pub const POLICY_ACTIONS: &[&str] = &[
    "search",
    "fetch",
    "shell",
    "coding",
    "write_file",
    "git",
    "lsp",
    "agent_run",
    "math",
    "board_list",
    "card_create",
    "card_routine",
    "card_routine_clear",
    "card_run",
    "card_agent",
    "card_image",
    "card_find",
    "done",
    "final",
    "answer",
    "message",
];
/// JSON keys whose string value is treated as an action word.
pub const POLICY_KEYS: [&str; 4] = ["tool", "action", "op", "verb"];

/// Policy vocabulary for tool-call actions: the known tool actions plus
/// every verb form in the modelless English corpus. An action word that is
/// neither is a hallucinated action — named in the repair feedback.
pub struct Policy {
    actions: HashSet<&'static str>,
    verbs: HashSet<&'static str>,
}

impl Default for Policy {
    fn default() -> Self {
        Self::new()
    }
}

impl Policy {
    pub fn new() -> Self {
        Policy {
            actions: POLICY_ACTIONS.iter().copied().collect(),
            verbs: modelless::corpus::words().collect(),
        }
    }

    /// Custom allowlist (e.g. a per-agent restricted tool set) on top of the
    /// verb corpus.
    pub fn from_actions(actions: &[&'static str]) -> Self {
        Policy {
            actions: actions.iter().copied().collect(),
            verbs: modelless::corpus::words().collect(),
        }
    }

    /// Words under the action keys that are neither a known tool action nor
    /// an English verb form from the corpus.
    pub fn violations(&self, value: &serde_json::Value) -> Vec<String> {
        let Some(obj) = value.as_object() else {
            return Vec::new();
        };
        let mut bad = Vec::new();
        for (key, val) in obj {
            let is_action_key = POLICY_KEYS.iter().any(|k| key.eq_ignore_ascii_case(k));
            let Some(word) = val.as_str() else {
                continue;
            };
            let word = word.split_whitespace().next().unwrap_or("");
            let word = word.to_lowercase();
            if word.is_empty() {
                continue;
            }
            if !is_action_key {
                continue;
            }
            let known = self.actions.contains(word.as_str()) || self.verbs.contains(word.as_str());
            if !known {
                bad.push(word);
            }
        }
        bad
    }
}

/// The process-wide policy (built once; the 3000-word corpus set is shared).
pub fn global_policy() -> &'static Policy {
    static POLICY: OnceLock<Policy> = OnceLock::new();
    POLICY.get_or_init(Policy::new)
}

/// Corrective feedback for an off-policy action word.
pub fn policy_prompt(violations: &[String]) -> String {
    format!(
        "{REPAIR_PREFIX}unknown action(s) [{}]: not a tool action and not an English verb. \
         Use one of: {} …",
        violations.join(", "),
        POLICY_ACTIONS[..8].join(", ")
    )
}

/// Corrective feedback for a stuck JSON call — sent back so the agent retries
/// with the exact defect named.
pub fn repair_prompt(stuck: &JsonStuck) -> String {
    let detail = match stuck {
        JsonStuck::NoJson => "your reply contained no JSON object at all.".to_owned(),
        JsonStuck::Truncated { depth } => format!(
            "your JSON was cut off mid-object ({depth} unclosed bracket(s)) — the call never closed."
        ),
        JsonStuck::Invalid { detail } => match extract_suggestion(detail) {
            Some(sugg) => format!("your JSON was rejected by the parser: {sugg}."),
            None => format!("your JSON was complete but syntactically invalid ({detail})."),
        },
        JsonStuck::NotObject => {
            "your JSON parsed but was not an object — tool calls must be a JSON object.".to_owned()
        }
    };
    format!("{REPAIR_PREFIX}{detail} {REPAIR_RETRY}")
}

/// Transcript pruner for the work loop: every tool result is an evictable
/// entry; when the budget is exceeded the katgpt usage-rate rule evicts the
/// coldest, oldest output first. Token counting defaults to the cheap
/// estimate; `with_counter` wires an exact counter (e.g. a kat gate).
pub struct ToolPruner {
    ctx: Context,
    count: Box<dyn Fn(&str) -> usize + Send>,
}

impl ToolPruner {
    pub fn new(budget_tokens: usize) -> Self {
        ToolPruner {
            ctx: Context::new(budget_tokens),
            count: Box::new(estimate_tokens),
        }
    }

    pub fn with_counter(mut self, count: Box<dyn Fn(&str) -> usize + Send>) -> Self {
        self.count = count;
        self
    }

    /// One transcript turn (label + text, e.g. `SHELL cargo check` + output).
    pub fn append(&mut self, label: &str, text: &str) -> u64 {
        let tokens = (self.count)(label) + (self.count)(text);
        self.ctx.push(Entry::new(
            EntryKind::Tool,
            format!("{label}: {text}"),
            tokens,
        ))
    }

    /// Pin an entry so eviction can never drop it (e.g. the current goal).
    pub fn pin(&mut self, label: &str, text: &str) -> u64 {
        let tokens = (self.count)(label) + (self.count)(text);
        self.ctx
            .push(Entry::new(EntryKind::Tool, format!("{label}: {text}"), tokens).pinned())
    }

    pub fn render(&mut self) -> String {
        self.ctx.render()
    }

    pub fn used_tokens(&self) -> usize {
        self.ctx.used_tokens()
    }

    pub fn len(&self) -> usize {
        self.ctx.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ctx.is_empty()
    }
}
