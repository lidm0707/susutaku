//! Speculative draft gate: katgpt-speculative bigram Markov table behind an
//! abstract [`DraftGate`], as used for draft-token windows.

use katgpt_speculative::bigram_markov::{BigramMarkovBuilder, BigramMarkovTable};

/// Draft-window tuning, mirroring the gguf-rs draft path.
#[derive(Debug, Clone, Copy)]
pub struct DraftConfig {
    /// Successors kept per bigram when building the table.
    pub top_m: usize,
    /// Draft tokens proposed per window.
    pub steps: usize,
    /// Stop drafting below this successor probability.
    pub min_prob: f32,
    /// Anti-repeat horizon: draft skips ids seen in the last N history tokens.
    pub anti_repeat: usize,
}

impl Default for DraftConfig {
    fn default() -> Self {
        Self {
            top_m: 8,
            steps: 4,
            min_prob: 0.3,
            anti_repeat: 8,
        }
    }
}

/// High-confidence continuation drafts built from token history.
pub trait DraftGate {
    /// Draft continuation ids after `last`, given the full history so far.
    fn draft(&mut self, hist: &[u32], last: u32) -> Vec<u32>;
}

/// Bigram Markov draft table over a fixed vocab.
pub struct BigramDraft {
    table: BigramMarkovTable,
    cfg: DraftConfig,
}

impl BigramDraft {
    /// Rebuild the table from the prompt history so far.
    pub fn build(hist: &[u32], vocab: usize, cfg: DraftConfig) -> Self {
        let mut b = BigramMarkovBuilder::new();
        b.add_sequence(hist);
        Self {
            table: b.build(vocab, cfg.top_m),
            cfg,
        }
    }

    pub const fn config(&self) -> &DraftConfig {
        &self.cfg
    }
}

impl DraftGate for BigramDraft {
    fn draft(&mut self, hist: &[u32], last: u32) -> Vec<u32> {
        let recent_start = hist.len().saturating_sub(self.cfg.anti_repeat);
        let mut draft = Vec::with_capacity(self.cfg.steps);
        let mut prev = last;
        for _ in 0..self.cfg.steps {
            let Some((succ, probs)) = self.table.successors(prev) else {
                break;
            };
            if probs[0] < self.cfg.min_prob || hist[recent_start..].contains(&succ[0]) {
                break;
            }
            draft.push(succ[0]);
            prev = succ[0];
        }
        draft
    }
}
