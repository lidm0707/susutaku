//! Bench: normal (HF tokenizers) vs katgpt BPE encode/decode on the
//! tokenizer.json of the first loadable model. Run with:
//!   cargo run -p susutaku-mlx --release --example tok_bench

use std::path::Path;
use std::time::Instant;

use susutaku_mlx::tok::{ChatTok, TokKind};

const PROMPT: &str = "<|im_start|>user\nExplain the difference between BPE and unigram tokenization in three sentences.<|im_end|>\n<|im_start|>assistant\n";

fn first_tokenizer_dir() -> Option<std::path::PathBuf> {
    ["models", "../models"].iter().find_map(|root| {
        std::fs::read_dir(root)
            .ok()?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.join("tokenizer.json").is_file())
    })
}

fn bench(kind: TokKind, dir: &Path, rounds: usize) -> (Vec<u32>, f64, String) {
    let tok = ChatTok::load(dir, kind).expect("load tokenizer");
    let mut ids = Vec::new();
    let t0 = Instant::now();
    for _ in 0..rounds {
        ids = tok.encode(PROMPT, true).expect("encode");
    }
    let secs = t0.elapsed().as_secs_f64() / rounds as f64;
    let text = tok.decode(&ids).expect("decode");
    (ids, secs * 1e3, text)
}

fn main() {
    let Some(dir) = first_tokenizer_dir() else {
        eprintln!("no tokenizer.json under ./models or ../models");
        std::process::exit(1);
    };
    println!("model dir: {}", dir.display());
    const ROUNDS: usize = 20;

    let (hf_ids, hf_ms, hf_text) = bench(TokKind::Normal, &dir, ROUNDS);
    let (kg_ids, kg_ms, kg_text) = bench(TokKind::Katgpt, &dir, ROUNDS);

    println!("prompt chars: {}", PROMPT.len());
    println!("normal : {:8.3} ms/encode · {} tokens", hf_ms, hf_ids.len());
    println!("katgpt : {:8.3} ms/encode · {} tokens", kg_ms, kg_ids.len());
    let same: usize = hf_ids
        .iter()
        .zip(kg_ids.iter())
        .filter(|(a, b)| a == b)
        .count();
    println!(
        "ids identical: {same}/hf={} kg={} (position match {:.1}%)",
        hf_ids.len(),
        kg_ids.len(),
        100.0 * same as f64 / hf_ids.len().max(1) as f64
    );
    println!(
        "roundtrip normal == source: {}",
        hf_text.trim_end() == PROMPT.trim_end()
    );
    println!(
        "roundtrip katgpt == source: {}",
        kg_text.trim_start_matches("<bos>").trim_end() == PROMPT.trim_end()
    );
}
