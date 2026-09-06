//! Debug: print top-5 last-token logits for "Say hello." to compare with the
//! mlx-lm reference. Run: cargo run --release -p susutaku-mlx --example dbg -- models/gemma-4-26b-a4b-it-4bit

use std::path::Path;

use tokenizers::Tokenizer;

type Ex<T> = Result<T, Box<dyn std::error::Error>>;

fn main() -> Ex<()> {
    let binding = std::env::args().nth(1).unwrap();
    let dir = Path::new(&binding);
    let tokenizer = Tokenizer::from_file(dir.join("tokenizer.json")).map_err(|e| e.to_string())?;
    let mut model = susutaku_mlx::engine::Model::load(dir)?;
    let ids = tokenizer
        .encode("Say hello.", true)
        .map_err(|e| e.to_string())?;
    let logits = model.logits_last(ids.get_ids())?;
    let mut top: Vec<(usize, f32)> = logits.iter().cloned().enumerate().collect();
    top.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (i, v) in top.iter().take(5) {
        println!("{i} {v}");
    }
    Ok(())
}
