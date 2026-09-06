# AGENTS.md

## Model policy (harness agent)

- Only load MLX models that are **larger than 30 GiB** on disk.
- Only use **4-bit (Q4) quantized** checkpoints, e.g. `-4bit` / `4bit` variants.
- First supported model: `qwen3.8 27b mlx` (`mlx-community/Qwen3.8-27B-4bit`, qwen3_5 hybrid).
- Second model: `gemma4 26b a4b it mlx` (`mlx-community/gemma-4-26b-a4b-it-4bit`, dense) — engine port complete, parity-verified vs mlx-lm.
- Model selection at runtime via `POST /api/models/select`; discovery goes through `hf_loader::loadable_models` — never hardcode model paths.
- Exception: `gemma4 e4b mlx` (`mlx-community/gemma-4-e4b-it-4bit`, ~4.8 GB) —
  supported despite being under the size floor (bring-up exception, plan 12).
- Local model dirs live in `models/` (see `models/names_models.md`).

## Layout

- `crates/core-agent` — agent state, workspace (macOS/Linux), web search
- `crates/hf_loader` — model dir scanning + loadability filter (size ≥ 30 GiB, q4, MLX)
- `crates/mlx-rs` — MLX inference backend
- `crates/pdf-rs` — PDF parsing
- `crates/work` — applications/services built on the crates above
- `piplines/`, `input/`, `output/`, `web_ui/` — pipeline and UI assets

## Rules

- Use **pure Rust** — no Python scripts, no shell-out glue where a Rust crate exists; Rust-only dependencies, no C/C++ build wrappers unless unavoidable
- Every finished task: `cargo check && cargo clippy`
- New plans go in `./.plans`, numbered 00–99
- Prefer enums over hard-coded values; constants for all magic numbers
