//! Minimal Qwen3.8 / qwen3_5 (MLX, 4-bit) example: hybrid gated-delta-net + full attention.
//!
//! Run: cargo run -p susutaku-mlx --example hello -- [model_dir|models_root]

use std::path::Path;

use susutaku_mlx::tok::{ChatTok, TokKind};

type Ex<T> = Result<T, Box<dyn std::error::Error>>;

fn main() -> Ex<()> {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "models".into());
    let root = Path::new(&arg);
    let dir = if root.join("config.json").is_file() {
        root.to_path_buf()
    } else {
        susutaku_mlx::loadable_model(root).ok_or_else(|| {
            "no loadable model found (MLX + 4-bit + ≤30 GiB policy) — pass a model dir".to_string()
        })?
    };
    println!("[INFO] model dir: {}", dir.display());

    let tokenizer = ChatTok::load(&dir, TokKind::Normal)?;
    let mut model = susutaku_mlx::engine::Model::load(&dir)?;
    println!("[INFO] loaded");
    let text = model.chat(&tokenizer, "Say hello.", 16)?;
    println!("{text}");
    Ok(())
}
