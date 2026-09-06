# Bench 15 — GGUF bigram speculative decode (katgpt-speculative::bigram_markov)

Model: Qwen3-8B-Q4_K_M, candle CPU backend, max_tokens=48, TEMP=0.7.
Drafter: katgpt-rs `BigramMarkovTable` built from the live history each
cycle; commit only chains with row prob ≥ 0.5 and no token repeated from
the last 8 history tokens; window verified in one batched forward.

| prompt | draft | tokens | tps |
|---|---|---|---|
| List the first ten prime numbers. | off | 48 | 10.75 |
| List the first ten prime numbers. | on | 48 | 10.90 |
| Count from 1 to 20, one number per line. | off | 48 | 10.01 |
| Count from 1 to 20, one number per line. | on | 48 | 9.81 |

Text output: identical with drafting on (guards hold).

## Also measured (earlier iteration, guards off)

| draft | tokens | tps |
|---|---|---|
| off | 44 | 10.77 |
| on | 48 | 13.14 |

Unguarded drafting repeated "A prime numbers." — 1.22× speed but corrupted
output. Not acceptable.

## Conclusion
On the candle CPU backend, a batched verify forward of k tokens costs
roughly the same FLOPs/time as k sequential steps, so exact speculative
verification is TPS-neutral here; the only speedup comes from skipping
verification, which trades quality. Drafting stays opt-in
(`chat_stats_with(.., true)`); the default path is plain decode.
Revisit on a GPU backend (Metal batch win) or if candle exposes
per-position logits for exact speculative sampling.
