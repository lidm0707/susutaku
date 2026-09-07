pub mod arch;
pub mod engine;
pub mod json_guard;
pub mod kv;
pub mod mem;
pub mod platform;
pub mod quant;
pub mod roofline;
pub mod tok;

pub fn loadable_model(root: &Path) -> Option<std::path::PathBuf> {
    hf_loader::loadable_models(root)
        .first()
        .map(|e| e.path.clone())
}

use std::path::Path;
