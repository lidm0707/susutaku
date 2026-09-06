//! GGUF inference engine (candle CPU backend). Same call shape as the MLX
//! model so the backend engine thread can run either interchangeably.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use candle_core::quantized::gguf_file;
use candle_core::{DType, Device, Tensor};
use candle_transformers::models::quantized_llama::ModelWeights as LlamaWeights;
use candle_transformers::models::quantized_qwen3::ModelWeights as Qwen3Weights;
use katgpt_speculative::bigram_markov::{BigramMarkovBuilder, BigramMarkovTable};
use susutaku_mlx::engine::{ChatTpl, GenStats};
use susutaku_mlx::tok::ChatTok;

const GGUF_EXT: &str = "gguf";
const ARCH_KEY: &str = "general.architecture";
pub const TEMP: f64 = 0.7;
pub const TOP_K: usize = 40;
const EOS_CANDIDATES: [&str; 6] = [
    "<|im_end|>",
    "<|eot_id|>",
    "</s>",
    "<|end_of_turn|>",
    "<|endoftext|>",
    "<eos>",
];
const CHATML_TPL: ChatTpl = ChatTpl::Chatml;

/// Speculative draft (bigram, from katgpt-rs bigram_markov): draft window and
/// the minimum row probability for a drafted token to be committed.
const DRAFT_STEPS: usize = 4;
const DRAFT_TOP_M: usize = 8;
const DRAFT_MIN_PROB: f32 = 0.5;
/// A drafted token already present in the last N history tokens is a bigram
/// repetition loop — dropped before it can feed itself.
const DRAFT_ANTI_REPEAT: usize = 8;

pub struct Model {
    weights: Weights,
    device: Device,
    eos_ids: Vec<u32>,
    vocab: u32,
}

enum Weights {
    Llama(LlamaWeights),
    Qwen3(Qwen3Weights),
}

impl Weights {
    /// Pick the loader from the GGUF `general.architecture` tag.
    fn from_gguf(
        content: gguf_file::Content,
        reader: &mut fs::File,
        device: &Device,
    ) -> Result<Self, String> {
        let arch = match content.metadata.get(ARCH_KEY) {
            Some(gguf_file::Value::String(s)) => s.clone(),
            _ => String::new(),
        };
        fn load<T>(arch: &str, w: Result<T, candle_core::Error>) -> Result<T, String> {
            w.map_err(|e| format!("failed to load {arch} weights: {e}"))
        }
        match arch.as_str() {
            "qwen3" => {
                load(&arch, Qwen3Weights::from_gguf(content, reader, device)).map(Self::Qwen3)
            }
            _ => load(&arch, LlamaWeights::from_gguf(content, reader, device)).map(Self::Llama),
        }
    }

    fn forward(&mut self, input: &Tensor, pos: usize) -> Result<Tensor, String> {
        match self {
            Self::Llama(w) => w.forward(input, pos),
            Self::Qwen3(w) => w.forward(input, pos),
        }
        .map_err(|e| e.to_string())
    }

    fn clear_kv_cache(&mut self) {
        match self {
            // candle's quantized_llama has no cache-reset API (private, append-only).
            Self::Llama(_) => {}
            Self::Qwen3(w) => w.clear_kv_cache(),
        }
    }
}

impl Model {
    /// A dir is supported when it holds a `.gguf` weight and a tokenizer.
    pub fn supported(dir: &Path) -> bool {
        gguf_weight(dir).is_some() && dir.join(susutaku_mlx::tok::TOKENIZER_FILE).is_file()
    }

    pub fn load(dir: &Path) -> Result<Self, String> {
        let file =
            gguf_weight(dir).ok_or_else(|| format!("no .gguf weight in {}", dir.display()))?;
        let device = Device::Cpu;
        let mut reader =
            fs::File::open(&file).map_err(|e| format!("failed to open {}: {e}", file.display()))?;
        let content = gguf_file::Content::read(&mut reader)
            .map_err(|e| format!("failed to read {}: {e}", file.display()))?;
        let vocab = gguf_vocab(&content);
        let weights = Weights::from_gguf(content, &mut reader, &device)?;
        Ok(Self {
            weights,
            device,
            eos_ids: eos_ids(dir),
            vocab,
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
        let started = Instant::now();
        self.weights.clear_kv_cache();
        let mut hist = tok.encode(prompt, true)?;
        let prompt_tokens = hist.len();
        let prompt_secs = started.elapsed().as_secs_f64();

        let mut generated = 0usize;
        let mut processed = hist.len();
        let mut next = sample(&self.forward(&hist, 0)?, TEMP)?;
        while generated < max_tokens && !self.is_eos(next) {
            hist.push(next);
            generated += 1;
            if generated >= max_tokens {
                break;
            }
            // `next` is sampled but not yet fed, so it opens every window.
            let drafted = if use_draft {
                self.draft_window(&hist, next)
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
            next = sample(&chunk_logits, TEMP)?;
        }
        let decode_secs = started.elapsed().as_secs_f64() - prompt_secs;
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
    fn draft_window(&self, hist: &[u32], last: u32) -> Vec<u32> {
        let mut b = BigramMarkovBuilder::new();
        b.add_sequence(hist);
        let table: BigramMarkovTable = b.build(self.vocab as usize, DRAFT_TOP_M);
        let recent_start = hist.len().saturating_sub(DRAFT_ANTI_REPEAT);
        let mut draft = Vec::with_capacity(DRAFT_STEPS);
        let mut prev = last;
        for _ in 0..DRAFT_STEPS {
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

/// Sample one token: temperature-scaled top-k on CPU, argmax at temp 0.
fn sample(logits: &Tensor, temp: f64) -> Result<u32, String> {
    let logits = logits
        .to_dtype(DType::F32)
        .and_then(|t| t.squeeze(0))
        .map_err(|e| e.to_string())?;
    if temp <= 0.0 {
        return logits
            .argmax(0)
            .and_then(|t| t.to_scalar::<u32>())
            .map_err(|e| e.to_string());
    }
    let mut probs = softmax(&logits)?;
    top_k_mask(&mut probs, TOP_K);
    let total: f32 = probs.iter().sum();
    let mut pick = rand::random::<f32>() * total;
    for (id, &p) in probs.iter().enumerate() {
        pick -= p;
        if pick <= 0.0 {
            return Ok(id as u32);
        }
    }
    probs
        .len()
        .checked_sub(1)
        .map(|i| i as u32)
        .ok_or_else(|| "empty logits".to_string())
}

fn softmax(logits: &Tensor) -> Result<Vec<f32>, String> {
    let max = logits
        .max(0)
        .and_then(|t| t.to_scalar::<f32>())
        .map_err(|e| e.to_string())?;
    let exp: Vec<f32> = logits
        .to_vec1::<f32>()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|&l| (l - max).exp())
        .collect();
    let sum: f32 = exp.iter().sum();
    Ok(exp.iter().map(|&e| e / sum).collect())
}

/// Keep the k highest-probability ids, zero the rest (top-k filter).
fn top_k_mask(probs: &mut [f32], k: usize) {
    if k == 0 || probs.len() <= k {
        return;
    }
    let mut order: Vec<usize> = (0..probs.len()).collect();
    order.sort_unstable_by(|&a, &b| probs[b].total_cmp(&probs[a]));
    for &i in &order[k..] {
        probs[i] = 0.0;
    }
}

/// Largest `.gguf` in the dir (single-file weights; shards are not merged).
fn gguf_weight(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == GGUF_EXT))
        .max_by_key(|p| fs::metadata(p).map(|m| m.len()).unwrap_or(0))
}

/// Vocabulary size from the GGUF metadata (arch-specific key).
fn gguf_vocab(content: &gguf_file::Content) -> u32 {
    ["qwen3.vocab_size", "llama.vocab_size"]
        .iter()
        .find_map(|k| content.metadata.get(*k).and_then(|v| v.to_u32().ok()))
        .unwrap_or(0)
}

/// Every EOS-candidate token id defined by the model's tokenizer.
fn eos_ids(dir: &Path) -> Vec<u32> {
    let Ok(tok) = ChatTok::load(dir, susutaku_mlx::tok::TokKind::Normal) else {
        return Vec::new();
    };
    EOS_CANDIDATES
        .iter()
        .filter_map(|s| tok.token_id(s))
        .collect()
}
