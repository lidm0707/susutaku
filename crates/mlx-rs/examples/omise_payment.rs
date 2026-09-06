//! Benchmark: prompt "code payment gateway omise in rust" — prefill TPS + avg decode TPS.
//!
//! Run: cargo run -p susutaku-mlx --example bench omise_payment -- [model_dir|models_root]

use std::path::Path;

use susutaku_mlx::engine;
use susutaku_mlx::tok::{ChatTok, TokKind};

type Ex<T> = Result<T, Box<dyn std::error::Error>>;

const PROMPT: &str = "code payment gateway omise in rust";
const MAX_TOKENS: usize = 256;

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
    let mut model = engine::Model::load(&dir)?;
    println!("[INFO] loaded, prompt: {PROMPT:?}");

    let (text, stats) = model.chat_stats(&tokenizer, PROMPT, MAX_TOKENS)?;
    println!("\n{text}\n");
    println!(
        "[BENCH] prefill: {} tokens in {:.2}s = {:.1} tps | decode avg: {} tokens in {:.2}s = {:.1} tps",
        stats.prompt_tokens,
        stats.prompt_secs,
        stats.prompt_tps(),
        stats.decode_tokens,
        stats.decode_secs,
        stats.decode_tps()
    );
    Ok(())
}
