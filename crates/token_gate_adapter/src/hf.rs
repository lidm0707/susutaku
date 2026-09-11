//! HF `tokenizers` backend for [`TokenGate`].

use crate::{GateErr, GateKind, GateResult, TokenGate};

pub struct HfGate {
    inner: tokenizers::Tokenizer,
}

impl HfGate {
    pub const fn new(inner: tokenizers::Tokenizer) -> Self {
        Self { inner }
    }
}

impl TokenGate for HfGate {
    fn kind(&self) -> GateKind {
        GateKind::Normal
    }

    fn encode(&self, text: &str, add_special: bool) -> GateResult<Vec<u32>> {
        let enc = self.inner.encode(text, add_special).gate()?;
        Ok(enc.get_ids().to_vec())
    }

    fn decode(&self, ids: &[u32]) -> GateResult<String> {
        self.inner.decode(ids, true).gate()
    }

    fn token_id(&self, token: &str) -> Option<u32> {
        self.inner.token_to_id(token)
    }
}
