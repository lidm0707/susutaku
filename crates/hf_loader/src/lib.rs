use std::fs;
use std::path::{Path, PathBuf};

const MODELS_DIR: &str = "models";
const GIB: u64 = 1024 * 1024 * 1024;
const MAX_MODEL_BYTES: u64 = 30 * GIB;
const QUANT_SUFFIX: &str = "-4bit";
const MLX_WEIGHTS: &str = "model.safetensors";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelFormat {
    Mlx,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub format: ModelFormat,
    pub quantized_4bit: bool,
}

impl ModelEntry {
    pub fn is_loadable(&self) -> bool {
        self.format == ModelFormat::Mlx && self.quantized_4bit && self.bytes <= MAX_MODEL_BYTES
    }
}

pub fn scan_local_models() -> Vec<ModelEntry> {
    scan_dir(Path::new(MODELS_DIR))
}

pub fn scan_dir(root: &Path) -> Vec<ModelEntry> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter_map(model_entry)
        .collect()
}

pub fn loadable_models(root: &Path) -> Vec<ModelEntry> {
    scan_dir(root)
        .into_iter()
        .filter(ModelEntry::is_loadable)
        .collect()
}

fn model_entry(path: PathBuf) -> Option<ModelEntry> {
    let name = path.file_name()?.to_str()?.to_string();
    let bytes = dir_size(&path)?;
    let format = if path.join(MLX_WEIGHTS).is_file() || has_sharded_weights(&path) {
        ModelFormat::Mlx
    } else {
        ModelFormat::Unknown
    };
    let quantized_4bit = name.contains(QUANT_SUFFIX);
    Some(ModelEntry {
        name,
        path,
        bytes,
        format,
        quantized_4bit,
    })
}

fn has_sharded_weights(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|it| {
            it.filter_map(Result::ok)
                .any(|e| e.file_name().to_string_lossy().starts_with("model-"))
        })
        .unwrap_or(false)
}

fn dir_size(dir: &Path) -> Option<u64> {
    let meta = fs::metadata(dir).ok()?;
    if !meta.is_dir() {
        return Some(meta.len());
    }
    let mut total = 0;
    for entry in fs::read_dir(dir).ok()? {
        let p = entry.ok()?.path();
        if p.is_dir() {
            total += dir_size(&p)?;
        } else {
            total += fs::metadata(&p).ok()?.len();
        }
    }
    Some(total)
}
