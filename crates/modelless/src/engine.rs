//! Engine: trains a katgpt BPE tokenizer over the verb corpus.

use crate::corpus;
use katgpt_tokenizer::{BpeTokenizer, BpeTokenizerImpl, BpeTrainer};

pub const VOCAB_SIZE: usize = 2048;
pub const MIN_WORD_LEN: usize = 2;

pub struct Engine {
    tokenizer: BpeTokenizer,
}

impl Engine {
    /// Train a BPE tokenizer over the full 3000-word corpus.
    pub fn train() -> Self {
        Self::train_with(VOCAB_SIZE)
    }

    pub fn train_with(vocab_size: usize) -> Self {
        let trained = BpeTrainer::train(&corpus::text(), vocab_size);
        Self { tokenizer: trained }
    }

    pub fn tokenizer(&self) -> &BpeTokenizer {
        &self.tokenizer
    }

    pub fn encode(&self, text: &str) -> Vec<usize> {
        BpeTokenizerImpl::encode(&self.tokenizer, text)
    }

    pub fn decode(&self, ids: &[usize]) -> String {
        BpeTokenizerImpl::decode(&self.tokenizer, ids)
    }

    /// Number of corpus words that survive a tokenize -> detokenize roundtrip.
    pub fn roundtrip_hits(&self) -> usize {
        corpus::words()
            .filter(|w| {
                if w.len() < MIN_WORD_LEN {
                    return true;
                }
                self.decode(&self.encode(w)) == *w
            })
            .count()
    }
}
