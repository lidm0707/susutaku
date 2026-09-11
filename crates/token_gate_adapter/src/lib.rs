//! Token gate adapter (TGA): abstract layer over tokenizer backends and
//! speculative draft tables.
//!
//! [`TokenGate`] abstracts encode/decode behind one trait with two
//! implementations: HF `tokenizers` ([`hf::HfGate`]) and katgpt BPE
//! ([`katgpt::KatgptGate`]). Both wrap the same `tokenizer.json`
//! vocab/merges, so token IDs stay compatible; the katgpt path differs only
//! in pretokenization strategy. [`draft::BigramDraft`] wraps
//! katgpt-speculative's bigram Markov table behind [`DraftGate`].

pub mod byte_map;
pub mod draft;
pub mod hf;
pub mod katgpt;

use std::path::Path;

pub use draft::{BigramDraft, DraftConfig, DraftGate};
pub use hf::HfGate;
pub use katgpt::KatgptGate;

pub const TOKENIZER_FILE: &str = "tokenizer.json";

/// Which tokenizer implementation decodes/encodes for a generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GateKind {
    #[default]
    Normal,
    Katgpt,
}

impl GateKind {
    /// API string → kind; anything unknown falls back to normal.
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("katgpt") => Self::Katgpt,
            _ => Self::Normal,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Katgpt => "katgpt",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateError(pub String);

impl std::fmt::Display for GateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GateError {}

pub type GateResult<T> = Result<T, GateError>;

trait GateErr<T> {
    fn gate(self) -> GateResult<T>;
}

impl<T, E: std::fmt::Display> GateErr<T> for Result<T, E> {
    fn gate(self) -> GateResult<T> {
        self.map_err(|e| GateError(e.to_string()))
    }
}

/// Abstract encode/decode gate over a shared vocab.
pub trait TokenGate: Send + Sync {
    fn kind(&self) -> GateKind;

    /// Encode `text`; `add_special` prepends BOS when the vocab defines one.
    fn encode(&self, text: &str, add_special: bool) -> GateResult<Vec<u32>>;

    fn decode(&self, ids: &[u32]) -> GateResult<String>;

    /// Look up one special-token id by content, when the vocab defines it.
    fn token_id(&self, token: &str) -> Option<u32>;
}

/// Load the gate for `kind` from a model dir holding `tokenizer.json`.
pub fn load(dir: &Path, kind: GateKind) -> GateResult<Box<dyn TokenGate>> {
    let file = dir.join(TOKENIZER_FILE);
    match kind {
        GateKind::Normal => {
            let tok = tokenizers::Tokenizer::from_file(&file)
                .map_err(|e| GateError(format!("failed to read {}: {e}", file.display())))?;
            Ok(Box::new(HfGate::new(tok)))
        }
        GateKind::Katgpt => {
            let bytes = std::fs::read(&file)
                .map_err(|e| GateError(format!("failed to read {}: {e}", file.display())))?;
            Ok(Box::new(KatgptGate::from_bytes(&bytes)?))
        }
    }
}
