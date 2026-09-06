//! Model dispatch, chat templating and generation loops (plain + n-gram
//! speculative). Architecture code lives in `qwen` / `gemma`.

use std::collections::HashMap;
use std::path::Path;

use mlx_rs::{
    Array, Dtype, array,
    error::Exception,
    ops::indexing::{IndexOp, NewAxis, argmax_axis},
    ops::multiply,
    random::{categorical, seed},
    transforms::eval,
};

const TEMP: f32 = 0.0;
const SEED: u64 = 0;
const SUPPORTED_MODEL_TYPE: &str = "qwen3_5";
const SUPPORTED_GEMMA: &str = "gemma4";
/// `<|tool_response>` id in the gemma generation_config stop set — never a
/// valid end-of-turn marker for plain chat.
const GEMMA_TOOL_RESPONSE_ID: u32 = 50;

/// Sink tokens always kept in Full-attention KV (StreamingLLM; attention
/// mass concentrates on the earliest tokens).
const KV_SINK: usize = 128;
/// Most recent KV positions kept alongside the sink.
const KV_KEEP: usize = 3840;
/// Compact only when the cache exceeds the budget by this hysteresis, so
/// the one-pass compaction amortizes over thousands of decode steps.
const KV_COMPACT_AT: usize = KV_SINK + KV_KEEP + 4224;

type Ex<T> = Result<T, Box<dyn std::error::Error>>;

/// When to halt generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// Stop at the model's EOS token (default).
    Eos,
    /// Additionally stop as soon as generated JSON balances (tool-call args);
    /// plain-text answers without `{`/`[` are unaffected.
    JsonComplete,
}

enum Arch {
    Qwen35(Box<crate::arch::qwen::Qwen35>),
    Gemma4(Box<crate::arch::gemma::Gemma>),
}

use crate::arch::gemma::AttnCache;
use crate::quant::{CONFIG_FILE, load_map, model_type};

pub struct Model {
    arch: Arch,
    /// EOS token ids from config.json — generation stops when one is sampled.
    eos_ids: Vec<u32>,
}

/// `eos_token_id` from config.json: a single id or a list; empty if absent.
/// Falls back to `generation_config.json` (some gemma repos only list the
/// full stop set there, e.g. `[1, 106, 50]`).
fn eos_ids(dir: &Path, json: &serde_json::Value) -> Vec<u32> {
    let from = |v: &serde_json::Value| match v {
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as u32))
            .collect(),
        serde_json::Value::Number(n) => n.as_u64().map(|v| vec![v as u32]).unwrap_or_default(),
        _ => Vec::new(),
    };
    let ids = from(&json["eos_token_id"]);
    if !ids.is_empty() {
        return ids;
    }
    let mut ids = ids;
    let file = match std::fs::read(dir.join("generation_config.json")) {
        Ok(f) => f,
        Err(_) => return ids,
    };
    let Ok(g) = serde_json::from_slice::<serde_json::Value>(&file) else {
        return ids;
    };
    ids.extend(from(&g["eos_token_id"]));
    ids
}

/// Chat prompt wrapper, selected by the loaded model's architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatTpl {
    /// Qwen ChatML: `<|im_start|>user ... <|im_end|><|im_start|>assistant`
    Chatml,
    /// Gemma turn format with a `<|think|>` system turn.
    Turn,
}

impl ChatTpl {
    /// Wrap a raw prompt body so the model answers it. `think` toggles the
    /// reasoning block (gemma system `<|think|>` turn / qwen prefilled empty
    /// think block).
    pub fn wrap(self, body: &str, think: bool) -> String {
        match self {
            Self::Chatml => {
                let opener = if think {
                    "<|im_start|>assistant\n"
                } else {
                    "<|im_start|>assistant\n<think>\n\n</think>\n"
                };
                format!("<|im_start|>user\n{body}<|im_end|>\n{opener}")
            }
            Self::Turn => {
                let system = if think {
                    "<|turn>system\n<|think|>\n<turn|>\n"
                } else {
                    ""
                };
                format!("{system}<|turn>user\n{body}<turn|>\n<|turn>model\n")
            }
        }
    }
}

impl Model {
    fn forward(&mut self, tokens: &Array) -> Ex<Array> {
        match &mut self.arch {
            Arch::Qwen35(m) => m.forward(tokens),
            Arch::Gemma4(m) => m.forward(tokens),
        }
    }

    fn reset(&mut self) {
        match &mut self.arch {
            Arch::Qwen35(m) => m.reset(),
            Arch::Gemma4(m) => m.reset(),
        }
    }

    /// Materialize every layer's packed KV arrays so the lazy per-step graph
    /// (dequant + concat + requant) never stacks up across decode steps.
    fn eval_caches(&self) -> Ex<()> {
        match &self.arch {
            Arch::Qwen35(m) => m.eval_caches(),
            Arch::Gemma4(m) => m.eval_caches(),
        }
    }

    /// Roll every layer's KV cache back to `kept` total positions
    /// (speculative-decode verification rollback).
    fn truncate_caches(&mut self, kept: usize) {
        match &mut self.arch {
            Arch::Qwen35(m) => m.caches.iter_mut().for_each(|c| c.truncate(kept)),
            Arch::Gemma4(m) => m.caches.iter_mut().for_each(|c| c.truncate(kept)),
        }
    }

    /// Sink+recent compaction of Full-attention KV once the cache outgrows
    /// `KV_COMPACT_AT` (qwen only; gemma window layers self-manage).
    fn compact_kv(&mut self) {
        match &mut self.arch {
            Arch::Qwen35(m) => m
                .caches
                .iter_mut()
                .for_each(|c| c.compact(KV_SINK, KV_KEEP).expect("kv compact failed")),
            Arch::Gemma4(_) => {}
        }
    }

    /// The chat prompt template for the loaded architecture.
    pub fn chat_tpl(&self) -> ChatTpl {
        match &self.arch {
            Arch::Qwen35(_) => ChatTpl::Chatml,
            Arch::Gemma4(_) => ChatTpl::Turn,
        }
    }

    /// Total cached positions (same for every layer within an arch).
    fn cached_len(&self) -> usize {
        match &self.arch {
            Arch::Qwen35(m) => m
                .caches
                .iter()
                .map(|c| match c {
                    crate::arch::qwen::Cache::Full(c) => c.k.len(),
                    crate::arch::qwen::Cache::Linear(Some((k, _))) => k.shape()[2] as usize,
                    crate::arch::qwen::Cache::Linear(None) => 0,
                })
                .max()
                .unwrap_or(0),
            Arch::Gemma4(m) => m
                .caches
                .iter()
                .map(|c| match c {
                    AttnCache::Full(c) => c.k.len(),
                    // Roll holds at most `window` positions; count is absolute
                    AttnCache::Roll(r) => r.count as usize,
                })
                .max()
                .unwrap_or(0),
        }
    }
}

fn sample(logits: &Array, temp: f32) -> Ex<Array> {
    if temp == 0.0 {
        Ok(argmax_axis(logits, -1, None)?)
    } else {
        let scaled = multiply(logits, &array!(1.0 / temp))?;
        Ok(categorical(scaled, None, None, None)?)
    }
}

impl Model {
    /// True when the model in `dir` uses an architecture this engine can run.
    pub fn supported(dir: &Path) -> bool {
        std::fs::read(dir.join(CONFIG_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .map(|json| {
                let mt = model_type(&json);
                mt == SUPPORTED_MODEL_TYPE || mt == SUPPORTED_GEMMA
            })
            .unwrap_or(false)
    }

    /// Load the model in `dir` (must contain config.json, tokenizer.json and safetensors).
    pub fn load(dir: &Path) -> Ex<Self> {
        crate::platform::log();
        crate::mem::init();
        seed(SEED)?;
        let json =
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(dir.join(CONFIG_FILE))?)?;
        match model_type(&json) {
            SUPPORTED_GEMMA => Ok(Model {
                arch: Arch::Gemma4(Box::new(crate::arch::gemma::Gemma::load(dir, &json)?)),
                eos_ids: eos_ids(dir, &json)
                    .into_iter()
                    // `<|tool_response>` (id 50) sits in the generation_config
                    // stop set but must not end a normal chat turn
                    .filter(|id| *id != GEMMA_TOOL_RESPONSE_ID)
                    .collect(),
            }),
            _ => Ok(Model {
                arch: Arch::Qwen35(Box::new(crate::arch::qwen::Qwen35::build(
                    &json,
                    &load_map(dir)?,
                )?)),
                eos_ids: eos_ids(dir, &json),
            }),
        }
    }

    pub fn chat(
        &mut self,
        tokenizer: &crate::tok::ChatTok,
        prompt: &str,
        max_tokens: usize,
    ) -> Ex<String> {
        let (text, _) = self.chat_stats(tokenizer, prompt, max_tokens)?;
        Ok(text)
    }

    /// Chat with an explicit stop condition (EOS-only by default).
    pub fn chat_stop(
        &mut self,
        tokenizer: &crate::tok::ChatTok,
        prompt: &str,
        max_tokens: usize,
        stop: Stop,
    ) -> Ex<(String, GenStats)> {
        self.chat_stats_stop(tokenizer, prompt, max_tokens, stop)
    }

    /// Last-token logits for the given ids (debug/parity testing).
    pub fn logits_last(&mut self, ids: &[u32]) -> Ex<Vec<f32>> {
        self.reset();
        let tokens = Array::from(ids).index(NewAxis);
        let logits = self.forward(&tokens)?;
        let last = logits.index((0, -1, ..));
        mlx_rs::transforms::eval(std::slice::from_ref(&last))?;
        Ok(last.as_slice().to_vec())
    }

    /// True when `id` is an EOS token for the loaded model.
    fn is_eos(&self, id: u32) -> bool {
        self.eos_ids.contains(&id)
    }

    /// Per-step guard: materialize the packed KV arrays every step so the
    /// lazy graph never stacks up, log a memory snapshot on the cadence, and
    /// enforce the resident-memory budget (clear the buffer cache first;
    /// abort generation only if live allocations alone still overflow).
    fn guard_step(&self, step: usize) -> Ex<()> {
        self.eval_caches()?;
        if step.is_multiple_of(crate::mem::MEM_CHECK_EVERY) {
            let (a, c, p) = crate::mem::snapshot_gib();
            eprintln!("[mem step {step}] active={a:.1} cache={c:.1} peak={p:.1} GiB");
        }
        if crate::mem::check().is_over() {
            return Err(Exception::custom(
                "memory budget exceeded with cache cleared; aborting generation",
            )
            .into());
        }
        Ok(())
    }

    /// Generate and return the text plus prefill/decode timing (for TPS benchmarks).
    pub fn chat_stats(
        &mut self,
        tokenizer: &crate::tok::ChatTok,
        prompt: &str,
        max_tokens: usize,
    ) -> Ex<(String, GenStats)> {
        self.chat_stats_stop(tokenizer, prompt, max_tokens, Stop::Eos)
    }

    pub fn chat_stats_stop(
        &mut self,
        tokenizer: &crate::tok::ChatTok,
        prompt: &str,
        max_tokens: usize,
        stop: Stop,
    ) -> Ex<(String, GenStats)> {
        let ids = tokenizer.encode(prompt, true)?;
        let tokens = Array::from(ids.as_slice()).index(NewAxis);

        self.reset();

        let t0 = std::time::Instant::now();
        let logits = self.forward(&tokens)?;
        eval(&[logits.index((.., -1, ..))])?;
        let prompt_secs = t0.elapsed().as_secs_f64();

        let mut y = sample(&logits.index((.., -1, ..)), TEMP)?;
        let mut out: Vec<Array> = Vec::with_capacity(max_tokens);
        let mut produced = 0usize;
        let mut guard = crate::json_guard::JsonGuard::new();

        let t1 = std::time::Instant::now();
        for i in 0..max_tokens {
            let id = y.item::<u32>();
            if self.is_eos(id) {
                break;
            }
            out.push(y.clone());
            produced += 1;
            if stop == Stop::JsonComplete {
                guard.feed(&tokenizer.decode(&[id])?);
                if guard.state() == crate::json_guard::GuardState::Balanced {
                    break;
                }
            }
            if self.cached_len() >= KV_COMPACT_AT {
                self.compact_kv();
            }
            let next = y.index((.., NewAxis));
            let logits = self.forward(&next)?;
            y = sample(&logits.index((.., -1, ..)), TEMP)?;
            self.guard_step(i)?;
            if i % 4 == 0 {
                eval(&out)?;
            }
        }
        eval(&out)?;
        let decode_secs = t1.elapsed().as_secs_f64();

        let ids: Vec<u32> = out.drain(..).map(|a| a.item::<u32>()).collect();
        let text = tokenizer.decode(&ids)?;
        let stats = GenStats {
            prompt_tokens: tokens.dim(1) as usize,
            prompt_secs,
            decode_tokens: produced,
            decode_secs,
        };
        Ok((text, stats))
    }

    /// Speculative decoding with prompt-lookup n-gram drafting.
    /// Greedy-exact: output is identical to the non-speculative loop.
    pub fn chat_speculative(
        &mut self,
        tokenizer: &crate::tok::ChatTok,
        prompt: &str,
        max_tokens: usize,
    ) -> Ex<(String, GenStats, usize)> {
        let ids = tokenizer.encode(prompt, true)?;
        let tokens = Array::from(ids.as_slice()).index(NewAxis);
        self.reset();

        let t0 = std::time::Instant::now();
        let logits = self.forward(&tokens)?;
        eval(&[logits.index((.., -1, ..))])?;
        let prompt_secs = t0.elapsed().as_secs_f64();
        let prompt_len = tokens.dim(1) as usize;

        let mut ngram = Ngram::new();
        ngram.observe(&ids);
        // Invariant: `context` = every position fed to the model, ending with
        // the pending `y` (already predicted, not yet in `out`).
        let mut y = sample(&logits.index((.., -1, ..)), TEMP)?;
        let mut context = ids.clone();
        context.push(y.item::<u32>());
        let mut out: Vec<Array> = Vec::with_capacity(max_tokens);
        let mut n_accepted: usize = 0;

        let t1 = std::time::Instant::now();
        while out.len() < max_tokens {
            let drafts = ngram.draft(&context);
            let steps = drafts.len() + 1; // drafts + one free token
            if drafts.is_empty() || out.len() + steps > max_tokens {
                // plain single-token step
                let id = y.item::<u32>();
                if self.is_eos(id) {
                    break;
                }
                out.push(y.clone());
                let next = y.index((.., NewAxis));
                let logits = self.forward(&next)?;
                y = sample(&logits.index((.., -1, ..)), TEMP)?;
                context.push(y.item::<u32>());
                ngram.observe(&context);
                if out.len().is_multiple_of(4) {
                    eval(&out)?;
                }
                if out.len().is_multiple_of(crate::mem::MEM_CHECK_EVERY) {
                    self.guard_step(out.len())?;
                } else {
                    self.eval_caches()?;
                }
                continue;
            }

            // verify [y] + drafts in one forward; `before` = cache positions
            // before this call
            let before = self.cached_len();
            let mut verify = vec![y.item::<u32>()];
            verify.extend_from_slice(&drafts);
            let vtok = Array::from_slice(&verify, &[1, verify.len() as i32]);
            let logits = self.forward(&vtok)?;
            eval(std::slice::from_ref(&logits))?;

            // argmax per position in one host read: row i predicts the token
            // after verify[i]
            let l2d = logits.reshape(&[verify.len() as i32, -1])?;
            let am = argmax_axis(&l2d, -1, None)?
                .reshape(&[-1])?
                .as_dtype(Dtype::Uint32)?;
            eval(std::slice::from_ref(&am))?;
            let am_ids: Vec<u32> = am.as_slice().to_vec();

            // accept the longest draft prefix the target agrees with
            let acc = drafts
                .iter()
                .zip(am_ids.iter())
                .take_while(|(d, a)| d == a && !self.is_eos(**d))
                .count();

            // keep y + accepted drafts; the next y is the target's own pick
            let one = |t: u32| Array::from_slice(&[t], &[1]);
            out.push(one(verify[0]));
            for d in drafts.iter().take(acc) {
                out.push(one(*d));
            }
            n_accepted += acc;
            y = one(am_ids[acc]);
            if self.is_eos(am_ids[acc]) {
                break;
            }

            // drop the rejected draft positions from every layer's cache
            self.truncate_caches(before + 1 + acc);

            // context: y was already in it; add accepted drafts + new y
            context.extend_from_slice(&drafts[..acc]);
            context.push(am_ids[acc]);
            ngram.observe(&context);
            eval(&out)?;
            self.guard_step(out.len())?;
        }
        eval(&out)?;
        let decode_secs = t1.elapsed().as_secs_f64();

        let ids: Vec<u32> = out.drain(..).map(|a| a.item::<u32>()).collect();
        let text = tokenizer.decode(&ids)?;
        let stats = GenStats {
            prompt_tokens: prompt_len,
            prompt_secs,
            decode_tokens: out.len(),
            decode_secs,
        };
        Ok((text, stats, n_accepted))
    }
}

// ---------- speculative decoding (prompt-lookup n-gram drafting) ----------

const NGRAM: usize = 3;
const MAX_DRAFT: usize = 4;

/// Maps the last `NGRAM` tokens to the token sequences that followed them
/// in (prompt + accepted output). Pure Rust, no second model needed.
struct Ngram {
    table: HashMap<Vec<u32>, Vec<u32>>,
}

impl Ngram {
    fn new() -> Self {
        Self {
            table: HashMap::new(),
        }
    }

    fn observe(&mut self, ids: &[u32]) {
        if ids.len() <= NGRAM {
            return;
        }
        for w in 0..ids.len() - NGRAM {
            let key = ids[w..w + NGRAM].to_vec();
            let entry = self.table.entry(key).or_default();
            let next = ids[w + NGRAM];
            if entry.last() != Some(&next) {
                entry.push(next);
            }
        }
    }

    /// Draft up to `MAX_DRAFT` tokens continuing from `context`.
    fn draft(&self, context: &[u32]) -> Vec<u32> {
        let mut out = Vec::with_capacity(MAX_DRAFT);
        if context.len() < NGRAM {
            return out;
        }
        let Some(follows) = self.table.get(&context[context.len() - NGRAM..]) else {
            return out;
        };
        let mut window: Vec<u32> = context[context.len() - NGRAM..].to_vec();
        let mut next = follows[0];
        while out.len() < MAX_DRAFT {
            out.push(next);
            window.remove(0);
            window.push(next);
            match self.table.get(&window) {
                Some(f) => next = f[0],
                None => break,
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GenStats {
    pub prompt_tokens: usize,
    pub prompt_secs: f64,
    pub decode_tokens: usize,
    pub decode_secs: f64,
}

impl GenStats {
    pub fn prompt_tps(&self) -> f64 {
        self.prompt_tokens as f64 / self.prompt_secs
    }

    pub fn decode_tps(&self) -> f64 {
        self.decode_tokens as f64 / self.decode_secs
    }
}
