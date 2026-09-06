//! Debug: specific logits + top-5 for ids from /tmp/ids.json.

use std::path::Path;

type Ex<T> = Result<T, Box<dyn std::error::Error>>;

fn main() -> Ex<()> {
    let binding = std::env::args().nth(1).unwrap();
    let dir = Path::new(&binding);
    let mut model = susutaku_mlx::engine::Model::load(dir)?;
    let ids: Vec<u32> = serde_json::from_slice(&std::fs::read("/tmp/ids.json")?)?;
    let logits = model.logits_last(&ids)?;
    println!("rs l100={:.3} l107={:.3} max={:.3}", logits[100], logits[107], logits.iter().cloned().fold(f32::MIN, f32::max));
    let mut top: Vec<(usize, f32)> = logits.iter().cloned().enumerate().collect();
    top.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (i, v) in top.iter().take(5) {
        println!("{i} {v}");
    }
    Ok(())
}
