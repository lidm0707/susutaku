# Bench 04 — katgpt-tokenizer vs normal (HF tokenizers)

Command: `cargo run -p susutaku-mlx --release --example tok_bench`
Model tokenizer: `models/gemma-4-26b-a4b-it-4bit/tokenizer.json`, 129-char ChatML prompt, 20 rounds avg.
Re-run after the ▁ space-marker fix (tok.rs, 2026-09).

| metric | normal (HF) | katgpt BPE |
| --- | --- | --- |
| encode latency | 0.012 ms | 0.065 ms |
| tokens produced | 37 | 38 |
| decode roundtrip | exact | exact |
| ids identical | — | 0/37 on ChatML prompt (HF maps `<|im_start|>` to special ids, katgpt merges them as text); plain-text prompts match 1:1 |

## History
- Before the fix katgpt produced 48 tokens (+30%) and dropped all spaces on
  decode: the GPT-2 `Ġ` byte map does not match gemma's SentencePiece `▁`
  (U+2581) vocab convention, so most merges fell to `unk`.

## Read
- katgpt encode is ~5× slower but <0.1 ms — negligible vs decode (seconds).
- Token counts now within ~3% of HF.
- Decode skips `added_tokens` special ids (no more `<|channel|>` / `<|turn>`
  leakage into chat output).
- Never mix: KV cache from a normal-encoded prompt + katgpt decode (or vice
  versa) — ids differ wherever special tokens or rarely-merged chars appear.
