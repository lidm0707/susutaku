# Compact: MoE (gemma4) vs dense-hybrid (qwen3.8) — same prompt, spec decode

Prompt: "What is the capital city of Thailand?", greedy, 64 decode tokens.
`chat_speculative` = prompt-lookup 3-gram drafting, MAX_DRAFT=4.

| Model | Arch | base decode | spec decode | spec verdict |
|---|---|---|---|---|
| gemma-4-26b-a4b-it-4bit | MoE A4B (KV only) | 25.8–29.3 | 24.5–28.4 | lossless, no win |
| Qwen3.8-27B-4bit | dense attn + linear (hybrid) | 25.3–25.6 | diverges | **broken — do not use** |

## Why

- **MoE no-win:** verify amortizes the weight *read*, but gemma4's top-8
  experts do per-token compute, so a 4-token verify costs ~4x FLOPs.
  Acceptance ~2-3 avg doesn't pay for it.
- **Qwen divergence:** its linear-attention layers keep a recurrent *state
  summary* (`Cache::Linear`), not position-indexed KV. Rolling back by
  truncation is mathematically invalid — state after N+k tokens ≠ state at N.
  KV-only rollback (gemma) is fine; hybrid models need state checkpointing or
  full recompute on rejection.

## Standing

- `chat_speculative`: correct on KV-only models (diff-verified), kept off the
  backend default path.
- Both attention paths (gemma `Attn`, qwen `Block`) now build masks that span
  cached + new positions — required for any multi-token verify forward.
- Revisit: dense KV-only model, or state checkpointing for the linear layers.
