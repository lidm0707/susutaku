# Rust test coverage — tooling + baseline

Tool: `cargo-llvm-cov` (0.8.7, llvm-tools installed). Run with `make cover`
(summary) or `make cover-html` (HTML report at `target/llvm-cov/html/`).

## Baseline (before test push)

Workspace total: **26.18% lines** (excluding kanban-rs, whose DB tests were
already failing on leftover Postgres rows before this work).

0% crates are hardware/model-bound (`mlx-rs` engine/arch, `gguf-rs` sample/
weights, `local_model` engine, `hf_loader`, `pdf-rs`, `work/pdf2csv`) — they
need real MLX/GPU models to execute; unit-testability is out of reach.

## After (same run scope)

| crate/file | before | after |
|---|---|---|
| piplines (all files) | 79–93% | **100%** |
| queue-rs (bounded/unbounded) | 90–94% | **100%** |
| prompt-sys (all files) | 93–100% | **100%** |
| proto-rs codec/server/client | 77% | **94–96%** (lines 91.2–96.9) |
| manager-rs manager_process | 72% | **91.4% regions / 95.5% lines** |
| backend infra/host_spec | untested | **97.1%** |
| workspace total (excl. kanban-rs) | 26.18% | **27.51% lines** |

## Documented unreachable exceptions (not deleted, no #[allow])

- codec.rs `serde_json::to_vec` / flush error arms — `Envelope` always
  serializes; flush fails only after a write failure.
- client.rs tiny-heartbeat write error — not deterministically triggerable.
- server.rs poisoned-RwLock arms, accept-error path — no user code inside the
  lock; serve owns the listener.
- manager_process.rs poisoned-lock fallbacks — unreachable via public API.
- host_spec.rs sysctl-failure fallback + Linux `cfg` branches on macOS runs.

## Cost

Full workspace instrumented run: ~2–3 min on M-series (build dominates).
Per-crate `-p` runs: seconds.
