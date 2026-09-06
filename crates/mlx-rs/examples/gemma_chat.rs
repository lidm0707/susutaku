//! Chat with the gemma4 model: greedy answer to a single prompt.
//! Run with:
//!   cargo run -p susutaku-mlx --release --example gemma_chat

use std::path::PathBuf;

use susutaku_mlx::engine::Model;
use susutaku_mlx::mem;
use susutaku_mlx::tok::{ChatTok, TokKind};

const MODEL_DIR: &str = "models/gemma-4-26b-a4b-it-4bit";
const ALT_MODEL_DIR: &str = "../models/gemma-4-26b-a4b-it-4bit";
const E4B_DIR: &str = "models/gemma-4-e4b-it-4bit";
const QWEN_DIR: &str = "models/Qwen3.8-27B-4bit";
const QWEN_ALT_DIR: &str = "../models/Qwen3.8-27B-4bit";
const PROMPT: &str = "What is the capital city of Thailand?";
const MAX_TOKENS: usize = 64;
/// Generation runs for the katgpt-rs runaway canary (plan 06 gate).
const RUNS: usize = 2;
/// Expected answer length the runaway ratio is measured against.
const TARGET_TOKENS: usize = 16;
/// Runaway gate thresholds (katgpt-core kv_eviction).Gemini r_max: median
/// output/target ratio must stay under 4x, and under half the runs may hit
/// the token cap.
const RUNAWAY_R_MAX: f32 = 4.0;
const RUNAWAY_P_CAP_MAX: f32 = 0.5;

fn user_prompt() -> String {
    std::env::var("PROMPT_TEXT").unwrap_or_else(|_| PROMPT.to_string())
}

/// Model selection: QWEN=1 env flag picks qwen3.8, E4B=1 picks the e4b,
/// default is gemma4 26b.
fn model_dirs() -> [PathBuf; 2] {
    if std::env::var("QWEN").is_ok() {
        [PathBuf::from(QWEN_DIR), PathBuf::from(QWEN_ALT_DIR)]
    } else if std::env::var("E4B").is_ok() {
        [PathBuf::from(E4B_DIR), PathBuf::from(E4B_DIR)]
    } else {
        [PathBuf::from(MODEL_DIR), PathBuf::from(ALT_MODEL_DIR)]
    }
}

/// Chat template (matches tokenizer.apply_chat_template with a
/// `<|think|>` system turn):
/// `<|turn>system\n<|think|>\n<turn|>\n<|turn>user ... <turn|>\n<|turn>model\n`
fn chat_prompt(user: &str) -> String {
    format!("<|turn>system\n<|think|>\n<turn|>\n<|turn>user\n{user}<turn|>\n<|turn>model\n")
}

fn model_dir() -> Option<PathBuf> {
    model_dirs()
        .into_iter()
        .find(|p| p.join("config.json").is_file())
}

fn main() {
    let Some(dir) = model_dir() else {
        eprintln!("gemma model not found under ./models or ../models");
        std::process::exit(1);
    };
    println!("model: {}", dir.display());
    let user = user_prompt();
    println!("prompt: {user}");

    let tokenizer = ChatTok::load(&dir, TokKind::Normal).expect("load tokenizer");
    let mut model = Model::load(&dir).expect("load model");

    let mut out_lens: Vec<usize> = Vec::new();
    for run in 0..RUNS {
        let (text, stats) = if std::env::var("SPEC").is_ok() {
            let (text, stats, accepted) = model
                .chat_speculative(&tokenizer, &chat_prompt(&user), MAX_TOKENS)
                .expect("generate");
            println!("[spec] accepted {accepted} drafted tokens");
            (text, stats)
        } else {
            model
                .chat_stats(&tokenizer, &chat_prompt(&user), MAX_TOKENS)
                .expect("generate")
        };
        if run == 0 {
            println!("\nanswer:{}", text.trim_end());
        }
        println!(
            "run {run}: {} tok in {:.2}s ({:.1} tok/s) · decode: {:.1} tok/s",
            stats.prompt_tokens,
            stats.prompt_secs,
            stats.prompt_tps(),
            stats.decode_tps()
        );
        out_lens.push(stats.decode_tokens);
    }

    let targets = vec![TARGET_TOKENS; out_lens.len()];
    let runaway =
        katgpt_core::kv_eviction::RunawayStats::from_generations(&out_lens, &targets, MAX_TOKENS);
    let ok = katgpt_core::kv_eviction::runaway_gate(&runaway, RUNAWAY_R_MAX, RUNAWAY_P_CAP_MAX);
    println!(
        "runaway gate: r_median={:.2} p_cap={:.2} n={} -> {}",
        runaway.r_median,
        runaway.p_cap,
        runaway.n,
        if ok { "PASS" } else { "FAIL" }
    );
    println!(
        "mem: active={:.1} GiB cache={:.1} GiB peak={:.1} GiB (budget {:.1})",
        mem::active_bytes() as f32 / GIB,
        mem::cache_bytes() as f32 / GIB,
        mem::peak_bytes() as f32 / GIB,
        (*mem::MEM_BUDGET_BYTES) as f32 / GIB
    );
}

const GIB: f32 = 1024.0 * 1024.0 * 1024.0;
