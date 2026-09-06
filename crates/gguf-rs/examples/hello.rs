//! Smoke-test the GGUF (candle CPU) engine: a hello prompt and a toolcall prompt.
//!
//! Run: cargo run -p gguf-rs --release --example hello -- [model_dir|models_root]

use std::path::Path;

use gguf_rs::Model;
use susutaku_mlx::tok::{ChatTok, TokKind};

type Ex<T> = Result<T, Box<dyn std::error::Error>>;

const HELLO_PROMPT: &str = "Say hello in one short sentence.";
const TOOL_PROMPT: &str =
    "What is the current CPU usage on this machine? Reply with only one tool line.";

fn main() -> Ex<()> {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "models".into());
    let root = Path::new(&arg);
    let dir = if Model::supported(root) {
        root.to_path_buf()
    } else {
        find_gguf(root).ok_or_else(|| {
            "no gguf model dir found under the models root — pass a model dir".to_string()
        })?
    };
    println!("[INFO] model dir: {}", dir.display());

    let tok = ChatTok::load(&dir, TokKind::Normal)?;
    let mut model = Model::load(&dir)?;
    println!("[INFO] loaded");

    for prompt in [HELLO_PROMPT, TOOL_PROMPT] {
        println!("\n=== prompt: {prompt:?}");
        let wrapped = model.chat_tpl().wrap(prompt, false);
        let (text, stats) = model.chat_stats(&tok, &wrapped, 64)?;
        println!("{text}");
        println!(
            "[stats] prompt={} tok ({:.2}s), decode={} tok ({:.2}s)",
            stats.prompt_tokens, stats.prompt_secs, stats.decode_tokens, stats.decode_secs
        );
    }
    Ok(())
}

/// First dir under `root` (one level deep) that gguf-rs can load.
fn find_gguf(root: &Path) -> Option<std::path::PathBuf> {
    let mut dirs: Vec<_> = std::fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| Model::supported(p))
        .collect();
    dirs.sort();
    dirs.into_iter().next()
}
