use std::path::{Path, PathBuf};

use susutaku_mlx::engine::Model;
use susutaku_mlx::tok::{ChatTok, TokKind};

pub const MODELS_ROOT: &str = "models";
pub const MAX_TOKENS: usize = 64;

pub fn active_model() -> Result<(Model, PathBuf), String> {
    let dir = susutaku_mlx::loadable_model(Path::new(MODELS_ROOT))
        .or_else(|| {
            std::fs::read_dir(MODELS_ROOT)
                .ok()?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| p.join("config.json").is_file())
        })
        .ok_or_else(|| "no model found".to_string())?;
    let model = Model::load(&dir).map_err(|e| e.to_string())?;
    Ok((model, dir))
}

pub fn ask(prompt: &str) -> Result<String, String> {
    let (mut model, dir) = active_model()?;
    let tokenizer = ChatTok::load(&dir, TokKind::Normal)?;
    model
        .chat(&tokenizer, prompt, MAX_TOKENS)
        .map_err(|e| e.to_string())
}
