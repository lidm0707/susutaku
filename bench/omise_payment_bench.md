# omise_payment bench — 2026-09-05

Prompt: "code payment gateway omise in rust"
Model: models/Qwen3.8-27B-4bit (~15 GiB, MLX, 4-bit, 27B)
Build: cargo run --release -p susutaku-mlx --example omise_payment
Max tokens: 256

| Phase  | Tokens | Time (s) | TPS  |
|--------|--------|----------|------|
| Prefill (prompt) | 7 | 0.39 | 17.9 |
| Decode (avg)     | 256 | 10.17 | 25.2 |

## Notes

- Prefill TPS is low because the prompt is only 7 tokens — the 0.39s is almost
  entirely model warm-up / graph compile, not throughput-bound. Longer prompts
  will show much higher prefill TPS.
- Decode avg ~25 tps on 27B 4-bit — this is the number that matters for
  interactive generation.
