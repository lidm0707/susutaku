# KV 3-bit quantization — memory + speed bench

Plan: `.plans/06-kv-quant-under-4bit.md` · Harness: `cargo run -p susutaku-mlx --release --example gemma_chat` (`QWEN=1` for qwen), run 2 (RUNS=2), gemma prompt "What is the capital city of Thailand?", MAX_TOKENS=64.

## Result

| config | decode tok/s | peak mem | verdict |
|---|---|---|---|
| bf16 KV baseline (pre-plan-06) | ~27 | n/a | reference |
| 3-bit KV, chunked v1 | 7–13 | **70 GiB spike** (Activity Monitor) | rejected |
| **3-bit KV, single-chunk + per-step eval + mem guard (final)** | **gemma 25–29 · qwen 24** | **gemma 13.4 GiB · qwen 14.8 GiB** | shipped |

Sample output (final, gemma):
```
run 0: decode 25.2 tok/s   run 1: decode 29.0 tok/s
runaway gate: r_median=3.12 p_cap=0.00 n=2 -> PASS
mem: active=13.2 GiB cache=0.3 GiB peak=13.4 GiB (budget 40.0)
```
Qwen (QWEN=1): decode 24.1 tok/s both runs, peak 14.8 GiB. Gate FAILed on
that run because the example hardcodes the gemma turn template for qwen
(`<|think|>` leaked, output ran to cap) — the backend applies the right
template per arch (`Model::chat_tpl`), and the gate correctly flagged the
mismatched-condition run.

## Mechanism notes

- 70 GiB was **not** stored KV: MLX's free-buffer cache + lazy-graph
  backlog. Fixes: single packed tensor per layer (O(1) graph nodes per
  step), `eval_caches()` once per decode step, `mlx_set_cache_limit(32 GiB)`
  at load.
- Budget guard (`mem.rs`): 40 GiB budget (10 GiB free on the 50 GiB
  machine); every 16 steps — over budget → `mlx_clear_cache()` → still over
  → generation aborts with a clean error.
- Requantize-per-append compounds mild rounding error (mlx-lm's own
  approach); outputs remain coherent. katgpt-core `runaway_gate` is the
  standing canary for longer-context regressions.
