//! Streaming JSON bracket guard (port of katgpt-validator's `PartialParser`,
//! reduced to the JSON subset: `{}`/`[]` depth + string/escape tracking).
//!
//! Multi-byte UTF-8 sequences never contain ASCII bytes, so feeding raw
//! decoded-token bytes is safe without re-assembly; a token split mid-codepoint
//! decodes lossily to U+FFFD, which is not a delimiter.

/// Verdict after feeding generated text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardState {
    /// All depths zero and no string open — the JSON value is complete.
    Balanced,
    /// Expecting more input (no `{`/`[` yet, or brackets still open).
    Open,
    /// A closing delimiter appeared without a matching opener.
    Broken,
}

const BRACE_OPEN: u8 = b'{';
const BRACE_CLOSE: u8 = b'}';
const BRACKET_OPEN: u8 = b'[';
const BRACKET_CLOSE: u8 = b']';
const QUOTE: u8 = b'"';
const BACKSLASH: u8 = b'\\';

#[derive(Debug, Clone, Copy, Default)]
pub struct JsonGuard {
    brace: i32,
    bracket: i32,
    in_string: bool,
    escape: bool,
    broken: bool,
    armed: bool,
}

impl JsonGuard {
    pub const fn new() -> Self {
        Self {
            brace: 0,
            bracket: 0,
            in_string: false,
            escape: false,
            broken: false,
            armed: false,
        }
    }

    /// Feed the next chunk of generated text (call once per decoded token).
    /// O(n) in chunk bytes, zero-copy.
    pub fn feed(&mut self, chunk: &str) {
        for &b in chunk.as_bytes() {
            if self.escape {
                self.escape = false;
                continue;
            }
            if self.in_string {
                match b {
                    QUOTE => self.in_string = false,
                    BACKSLASH => self.escape = true,
                    _ => {}
                }
                continue;
            }
            match b {
                BRACE_OPEN => {
                    self.brace += 1;
                    self.armed = true;
                }
                BRACKET_OPEN => {
                    self.bracket += 1;
                    self.armed = true;
                }
                BRACE_CLOSE => {
                    self.brace -= 1;
                    if self.brace < 0 {
                        self.broken = true;
                    }
                }
                BRACKET_CLOSE => {
                    self.bracket -= 1;
                    if self.bracket < 0 {
                        self.broken = true;
                    }
                }
                QUOTE => self.in_string = true,
                _ => {}
            }
        }
    }

    pub fn state(&self) -> GuardState {
        if self.broken {
            GuardState::Broken
        } else if !self.armed || self.in_string || self.brace > 0 || self.bracket > 0 {
            GuardState::Open
        } else {
            GuardState::Balanced
        }
    }
}
