# Speculative decoding (prompt-lookup, n-gram) — gemma4 bench

Model: `models/gemma-4-26b-a4b-it-4bit`, greedy, 64 decode tokens.
Drafter: 3-gram over prompt+output, `MAX_DRAFT=4`.

## Results

| Prompt | Mode | decode tok/s | accepted |
|---|---|---|---|
| capital of Thailand (Q&A) | base | 25.8 | — |
| capital of Thailand (Q&A) | spec | 24.5 | 5 |
| repeat list back (copy) | base | 28.2–29.3 | — |
| repeat list back (copy) | spec | 25.3–28.4 | 10 |

Correctness: speculative output **token-identical** to baseline on both prompts
(greedy-exact verified by diff).

## Findings

1. **No speed win on this MoE model.** Dense models amortize the single
   weight-read across the verify batch, so verify is nearly free. Gemma4's
   A4B MoE pays real per-token expert compute (top-8 routers run per token),
   so a 5-token verify costs ~5x the FLOPs of one step; acceptance ≤ 3 avg
   does not pay for it.
2. Host-read reduction (one `as_slice` for the argmax row instead of 5
   `.item()` syncs) helped but did not flip the sign.
3. Prompt-lookup stays useful where acceptance is very high (verbatim
   copying/editing loops); chat Q&A does not reach that bar.

## Conclusion

Keep `chat_speculative` (lossless, off by default via `SPEC=1` in
`examples/gemma_chat.rs`). Do not wire it into the backend default path for
gemma4. Revisit with: (a) dense models (qwen3.8 draft may differ), (b) tree
verification, or (c) a cheap draft model sharing the tokenizer.
