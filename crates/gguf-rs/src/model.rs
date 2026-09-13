use std::fs;
use std::path::Path;
use std::time::Instant;

use candle_core::{Device, Tensor};

use crate::TEMP;
use crate::sample::sample;
use crate::weights;
use susutaku_mlx::stats::GenStats;
use susutaku_mlx::tok::ChatTok;
use susutaku_mlx::tpl::ChatTpl;

use katgpt_speculative::acceptance_forecast::AcceptanceForecast;

const CHATML_TPL: ChatTpl = ChatTpl::Chatml;

/// Speculative draft (bigram, from katgpt-rs bigram_markov): draft window and
/// the minimum row probability for a drafted token to be committed.
pub const DRAFT_STEPS: usize = 4;
const DRAFT_TOP_M: usize = 8;
const DRAFT_MIN_PROB: f32 = 0.5;
/// A drafted token already present in the last N history tokens is a bigram
/// repetition loop — dropped before it can feed itself.
const DRAFT_ANTI_REPEAT: usize = 8;
/// Entropy→acceptance forecast (katgpt-rs AcceptanceForecast, Bebop bound
/// α ≈ a − b·H): slope and the gate below which drafting is skipped — a
/// high-entropy span makes bigram drafts junk faster than they decode.
const DRAFT_ALPHA_B: f32 = 0.2;
const DRAFT_ALPHA_DECAY: f32 = 0.1;
pub const DRAFT_MIN_ALPHA: f32 = 0.35;

/// Every EOS-candidate token id defined by the model's tokenizer.
fn eos_ids(dir: &Path) -> Vec<u32> {
    let Ok(tok) = ChatTok::load(dir, susutaku_mlx::tok::TokKind::Normal) else {
        return Vec::new();
    };
    crate::EOS_CANDIDATES
        .iter()
        .filter_map(|s| tok.token_id(s))
        .collect()
}

/// Draft window length scaled by the forecast acceptance rate: a middling α
/// shrinks the window instead of gambling the full DRAFT_STEPS on it.
pub fn adaptive_draft_len(alpha: f32) -> usize {
    ((DRAFT_STEPS as f32 * alpha).ceil() as usize).clamp(1, DRAFT_STEPS)
}

/// Last-position logits as an owned f32 vec (the forecast's input shape).
fn logits_f32(t: &Tensor) -> Result<Vec<f32>, String> {
    t.to_dtype(candle_core::DType::F32)
        .and_then(|t| t.squeeze(0))
        .and_then(|t| t.to_vec1())
        .map_err(|e| e.to_string())
}

pub struct Model {
    weights: weights::Weights,
    device: Device,
    eos_ids: Vec<u32>,
    vocab: u32,
    forecast: AcceptanceForecast,
}

impl Model {
    /// A dir is supported when it holds a `.gguf` weight and a tokenizer.
    pub fn supported(dir: &Path) -> bool {
        weights::gguf_weight(dir).is_some() && dir.join(susutaku_mlx::tok::TOKENIZER_FILE).is_file()
    }

    pub fn load(dir: &Path) -> Result<Self, String> {
        let file = weights::gguf_weight(dir)
            .ok_or_else(|| format!("no .gguf weight in {}", dir.display()))?;
        let device = Device::Cpu;
        let mut reader =
            fs::File::open(&file).map_err(|e| format!("failed to open {}: {e}", file.display()))?;
        let content = candle_core::quantized::gguf_file::Content::read(&mut reader)
            .map_err(|e| format!("failed to read {}: {e}", file.display()))?;
        let vocab = weights::gguf_vocab(&content);
        let weights = weights::Weights::from_gguf(content, &mut reader, &device)?;
        Ok(Self {
            weights,
            device,
            eos_ids: eos_ids(dir),
            vocab,
            forecast: AcceptanceForecast::with_params(1.0, DRAFT_ALPHA_B, DRAFT_ALPHA_DECAY),
        })
    }

    pub fn chat_tpl(&self) -> ChatTpl {
        CHATML_TPL
    }

    /// Generate up to `max_tokens` after `prompt`, returning text + stats.
    ///
    /// Bigram-drafted windows are verified in one batched forward per window:
    /// a drafted token is kept when its bigram probability clears
    /// `DRAFT_MIN_PROB`, and the model's own sampled token after the window
    /// anchors every cycle (see plan 15 for the acceptance trade-off).
    pub fn chat_stats(
        &mut self,
        tok: &ChatTok,
        prompt: &str,
        max_tokens: usize,
    ) -> Result<(String, GenStats), String> {
        // Drafting is opt-in: bench 15 shows CPU-neutral TPS with guards on.
        self.chat_stats_with(tok, prompt, max_tokens, false)
    }

    /// `use_draft = false` runs plain token-by-token decode (bench baseline).
    pub fn chat_stats_with(
        &mut self,
        tok: &ChatTok,
        prompt: &str,
        max_tokens: usize,
        use_draft: bool,
    ) -> Result<(String, GenStats), String> {
        self.weights.clear_kv_cache();
        self.forecast.reset_ema();
        let mut hist = tok.encode(prompt, true)?;
        let prompt_tokens = hist.len();

        let t0 = Instant::now();
        let mut generated = 0usize;
        let mut processed = hist.len();
        let first = self.forward(&hist, 0)?;
        self.forecast.observe_and_forecast(&logits_f32(&first)?);
        let mut next = sample(&first, TEMP)?;
        let prompt_secs = t0.elapsed().as_secs_f64();
        while generated < max_tokens && !self.is_eos(next) {
            hist.push(next);
            generated += 1;
            if generated >= max_tokens {
                break;
            }
            // Low forecast acceptance skips the window entirely and a
            // middling one shortens it (plan 15: bench showed CPU-neutral
            // drafting when gated only by the static flag).
            let alpha = self.forecast.forecast_alpha_current();
            let drafted = if use_draft && alpha >= DRAFT_MIN_ALPHA {
                self.draft_window(&hist, next, adaptive_draft_len(alpha))
            } else {
                Vec::new()
            };
            let (chunk_logits, window) = if drafted.is_empty() {
                let logits = self.forward(&[next], processed)?;
                (logits, 1usize)
            } else {
                let mut chunk = Vec::with_capacity(1 + drafted.len());
                chunk.push(next);
                chunk.extend_from_slice(&drafted);
                let logits = self.forward(&chunk, processed)?;
                hist.extend_from_slice(&drafted);
                generated += drafted.len();
                (logits, chunk.len())
            };
            processed += window;
            // Observe the distribution this cycle's `next` was drawn from.
            self.forecast
                .observe_and_forecast(&logits_f32(&chunk_logits)?);
            next = sample(&chunk_logits, TEMP)?;
        }
        let decode_secs = t0.elapsed().as_secs_f64() - prompt_secs;
        Ok((
            tok.decode(&hist[prompt_tokens..])?,
            GenStats {
                prompt_tokens,
                prompt_secs,
                decode_tokens: generated,
                decode_secs,
            },
        ))
    }

    /// High-confidence bigram continuation of `last` from the history so far.
    fn draft_window(&self, hist: &[u32], last: u32, steps: usize) -> Vec<u32> {
        use katgpt_speculative::bigram_markov::{BigramMarkovBuilder, BigramMarkovTable};

        let mut b = BigramMarkovBuilder::new();
        b.add_sequence(hist);
        let table: BigramMarkovTable = b.build(self.vocab as usize, DRAFT_TOP_M);
        let recent_start = hist.len().saturating_sub(DRAFT_ANTI_REPEAT);
        let mut draft = Vec::with_capacity(steps);
        let mut prev = last;
        for _ in 0..steps {
            let Some((succ, probs)) = table.successors(prev) else {
                break;
            };
            if probs[0] < DRAFT_MIN_PROB || hist[recent_start..].contains(&succ[0]) {
                break;
            }
            draft.push(succ[0]);
            prev = succ[0];
        }
        draft
    }

    fn forward(&mut self, ids: &[u32], pos: usize) -> Result<Tensor, String> {
        let input = Tensor::new(ids, &self.device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| e.to_string())?;
        let out = self.weights.forward(&input, pos)?;
        out.squeeze(0).map_err(|e| e.to_string())
    }

    fn is_eos(&self, id: u32) -> bool {
        self.eos_ids.contains(&id)
    }
}
