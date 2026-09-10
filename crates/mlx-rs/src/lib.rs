// Native-MLX modules: the mlx C++ backend only builds on Apple Silicon.
#[cfg(target_os = "macos")]
pub mod arch;
#[cfg(target_os = "macos")]
pub mod engine;
pub mod json_guard;
#[cfg(target_os = "macos")]
pub mod kv;
#[cfg(target_os = "macos")]
pub mod mem;
#[cfg(target_os = "macos")]
pub mod platform;
#[cfg(target_os = "macos")]
pub mod quant;
pub mod roofline;
pub mod stats;
pub mod tok;
pub mod tpl;

pub fn loadable_model(root: &Path) -> Option<std::path::PathBuf> {
    hf_loader::loadable_models(root)
        .first()
        .map(|e| e.path.clone())
}

use std::path::Path;
