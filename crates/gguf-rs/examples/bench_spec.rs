//! Bench: bigram-drafted speculative decode vs plain decode TPS (GGUF, CPU).
//!
//! Run: cargo run -p gguf-rs --release --example bench_spec -- [model_dir]

use std::path::Path;

use gguf_rs::Model;
use susutaku_mlx::engine::ChatTpl;
use susutaku_mlx::tok::{ChatTok, TokKind};

type Ex<T> = Result<T, Box<dyn std::error::Error>>;

const PROMPT: &str = "List the first ten prime numbers.";
const REPEAT_PROMPT: &str = "Count from 1 to 20, one number per line.";
const MAX_TOKENS: usize = 48;

fn main() -> Ex<()> {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "models".into());
    let dir = Path::new(&arg);

    let tok = ChatTok::load(dir, TokKind::Normal)?;
    let mut model = Model::load(dir)?;
    println!(
        "[INFO] model dir: {}, max_tokens={MAX_TOKENS}",
        dir.display()
    );

    let mut lines = Vec::new();
    for prompt in [PROMPT, REPEAT_PROMPT] {
        let wrapped = ChatTpl::Chatml.wrap(prompt, false);
        for use_draft in [false, true] {
            let (text, stats) = model.chat_stats_with(&tok, &wrapped, MAX_TOKENS, use_draft)?;
            let tps = stats.decode_tokens as f64 / stats.decode_secs;
            println!("\n--- prompt={prompt:?} draft={use_draft} tps={tps:.2}");
            println!("{text}");
            lines.push(format!(
                "| {} | {} | {} | {:.2} |",
                prompt,
                if use_draft { "on" } else { "off" },
                stats.decode_tokens,
                tps
            ));
        }
    }
    println!("\n| prompt | draft | tokens | tps |");
    println!("|---|---|---|---|");
    for l in lines {
        println!("{l}");
    }
    Ok(())
}
