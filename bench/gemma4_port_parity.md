# gemma4 engine port — parity & debug notes (2026-09-06)

## Root cause found this session
`mx.fast.rope` (non-traditional) pairs dim `i` with `i + dims/2` where `dims` is the
`dims` ARGUMENT — not `i + rotated/2`. The old PropRope paired `i ↔ i+rotated/2`,
which is only correct when `rotated == dims` (the sliding layers). For the full
layers (dims=512, rotated=128) every rotated pair was wrong. Fixed by splitting at
`dims/2` and giving angle 0 (cos=1, sin=0) to pairs `>= rotated/2`.

Second bug: `argpartition_axis` output slice `part[..., -8:]` is NON-CONTIGUOUS.
`Array::as_slice::<u32>()` on a non-contiguous array returns garbage reinterpreted
memory — the host-side MoE grouping silently read wrong expert ids. Fixed by
`reshape` (forces a contiguous copy) before any host read. Rule of thumb: never
`as_slice` a sliced/indexed array without making it contiguous first.

## Third bug (comparison artifact, not model bug)
The layer debug prints used `h.index((0, 0, ..4))` — position 0 — while the python
reference printed `h[0, -1, :4]` — last position. Position 0 matches even when the
model is wrong (rope identity at pos 0 + peaked attention), which sent the previous
session chasing layer-0 divergence that did not exist at the last token.

## Parity result (prompt "What is 2+2?", 23 tokens, chat template)
- All 30 layer last-token outputs match mlx-lm to bf16 noise (worst dim diff 0.19
  on late layers; expected given bf16 rounding amplified through 128-expert MoE).
- Final logits top-5: {100, 236820, 8454, 5596, 10810} — same set as mlx-lm,
  top-1 = token 100 at logit 25.69 vs py 25.625.

## Validation performed
- Rope unit test vs `mx.fast.rope` (base and freqs paths), positions 0–3, both
  sliding (dims=rotated=256, θ=1e4) and full (dims=512, rotated=128, θ=1e6):
  exact match. `crates/mlx-rs/examples/rope.rs` (kept as a regression check).
- Router: top-8 indices + softmax×per_expert_scale weights match exactly.
- o_proj / expert e85 / flat-row dumps vs python: match.
- `cargo check` + `cargo clippy` clean (0 warnings after --fix).

## Perf (not yet measured)
Decode tok/s gemma4 vs qwen3.8 still TODO — the parity harness is single-forward.
Suggested bench: 32-token greedy decode from the same prompt, wall-clock tok/s.
