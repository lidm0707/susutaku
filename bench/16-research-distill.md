# Bench 16 — research-rs distill session (expected cost profile)

Mechanism: `DistillSession` = katgpt `SelfDistillingBandit` over
`NoScreeningPruner` bandit, UCB1, 8 arms, in-memory episodes.

Guess (to verify when wired to a live model):

| operation | expected cost |
| --- | --- |
| `match_ratio` (LCS-based, ~50 tokens) | ~10 µs per update |
| `observe` (episode hit + bandit update) | ~20–50 µs |
| `observe` (episode miss) | ~5 µs (pure acceptance path) |
| `best_arm` for warm domain | ~1 µs |
| memory | ~num_arms × domains × 8 B for Q-values |

## Read plan
- Agent loop calls `observe` once per generation — bandwidth trivial next to
  a forward pass (seconds). No hot-path concern.
- If episode store grows past ~10⁴ entries, `MemoryEpisodeLookup` (linear Vec
  scan) becomes the bottleneck — swap in a hash-backed lookup then.

## Validate
- `cargo test -p research-rs` covers correctness, not timing; add criterion
  loop only if a real workload shows the observe path matters.
