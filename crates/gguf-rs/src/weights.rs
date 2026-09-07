use std::fs;
use std::path::{Path, PathBuf};

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::models::quantized_llama::ModelWeights as LlamaWeights;
use candle_transformers::models::quantized_qwen3::ModelWeights as Qwen3Weights;

use crate::GGUF_EXT;

pub const ARCH_KEY: &str = "general.architecture";
const VOCAB_KEYS: [&str; 2] = ["qwen3.vocab_size", "llama.vocab_size"];

pub enum Weights {
    Llama(LlamaWeights),
    Qwen3(Qwen3Weights),
}

impl Weights {
    /// Pick the loader from the GGUF `general.architecture` tag.
    pub fn from_gguf(
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

    pub fn forward(&mut self, input: &Tensor, pos: usize) -> Result<Tensor, String> {
        match self {
            Self::Llama(w) => w.forward(input, pos),
            Self::Qwen3(w) => w.forward(input, pos),
        }
        .map_err(|e| e.to_string())
    }

    pub fn clear_kv_cache(&mut self) {
        match self {
            // candle's quantized_llama has no cache-reset API (private, append-only).
            Self::Llama(_) => {}
            Self::Qwen3(w) => w.clear_kv_cache(),
        }
    }
}

/// Largest `.gguf` in the dir (single-file weights; shards are not merged).
pub fn gguf_weight(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == GGUF_EXT))
        .max_by_key(|p| fs::metadata(p).map(|m| m.len()).unwrap_or(0))
}

/// Vocabulary size from the GGUF metadata (arch-specific key).
pub fn gguf_vocab(content: &gguf_file::Content) -> u32 {
    VOCAB_KEYS
        .iter()
        .find_map(|k| content.metadata.get(*k).and_then(|v| v.to_u32().ok()))
        .unwrap_or(0)
}
