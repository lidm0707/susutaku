# vLLM → local_model distillation

Research: vLLM architecture overview (docs.vllm.ai, V1), PagedAttention kernel doc, and the original PagedAttention blog (2023-06-20). This doc maps each vLLM idea to our `local_model` + `mlx-rs` reality and marks what is worth adopting, adapting, or skipping.

## What local_model is today (baseline for comparison)

- One OS thread per loaded model (`engine.rs`), model is non-Send, jobs arrive over a `std::sync::mpsc` channel.
- **One job at a time** — `serve_jobs` loops `rx.recv()` and runs full generate before the next job. No batching.
- No KV cache reuse across requests; each job re-prefills the whole prompt.
- No streaming; the reply is a single `GenReply` on a oneshot.
- `ModelPool` swaps engines under `RwLock` (old engine thread lives on until channel close — minor leak, see notes).
- OpenAI-compatible surface (`/v1/chat/completions`) exists at the backend client level (plan 43), not streaming.

## vLLM techniques, distilled

### 1. PagedAttention — block-table KV cache

vLLM's core win: store KV per fixed-size **block** (e.g. 16 tokens), map logical→physical blocks via a block table, allocate on demand. Waste drops from 60–80% (fragmentation + over-reservation) to <4% (only the last block is partially used). Blocks are refcounted and copy-on-write shared, which makes parallel sampling/beam search cheap (up to 55% memory cut).

**For us:** MLX's unified memory removes the "fit more sequences in scarce VRAM" pressure that motivated PagedAttention, and we run one sequence at a time. Full paged attention is over-engineering. What *is* worth taking:

- **Block-quantized KV accounting**: track KV bytes per block so `GenStats` can report KV footprint; groundwork for any future batching (see bench/kv_quant.md).
- If we ever add multi-request serving on one model, adopt the block table directly rather than contiguous per-sequence caches — retrofitting later is much harder.

### 2. Continuous batching (iteration-level scheduling)

vLLM V1's engine core runs a busy loop that re-decides **every decode step** which sequences run: finished sequences leave, waiting prefills join mid-flight. This is the single biggest throughput lever (with PagedAttention) and is independent of GPU brand.

**For us:** highest-value idea here. Today a long generation head-of-line blocks every other job; a 1-token request behind a max-tokens job waits minutes. Minimal Rust adoption path:

- Engine thread owns a small `Vec<Job>` of active sequences; each loop step does one token per active job, removes finished ones, admits queued ones up to a `MAX_ACTIVE_SEQUENCES` const.
- MLX supports batched forward passes; even round-robin single-sequence stepping (batch=1 per step) removes head-of-line blocking without touching kernels. True batched decode (batch=n) is the second step.

### 3. Prefix caching

vLLM hashes prompt blocks and reuses KV for shared prefixes (same system prompt, multi-turn history, agent scaffolding). Huge for agentic workloads (vLLM's own AgentX post: agents share long, repeated prefixes).

**For us:** our primary client is the agent backend — every run re-sends the same system prompt + history. Block-level prefix KV reuse across jobs is the top *latency* win available: prefill drops from O(full prompt) to O(new tokens). Requires the KV cache to outlive a job (hold it in the engine thread keyed by prompt-prefix hash) — feasible precisely because our model already lives in one dedicated thread. Pair with turn-level API: accept `messages: [..]` so the engine can hash stable prefixes instead of one flattened string.

### 4. Chunked prefill

Long prompts are split into chunks interleaved with decode steps so a big prefill doesn't stall running decodes.

**For us:** only matters once continuous batching exists. On Metal with batch=1 round-robin it's nearly free: prefill the queued job in fixed token chunks (`PREFILL_CHUNK_TOKENS`) between other jobs' decode steps.

### 5. Speculative decoding

Draft model proposes k tokens, target verifies in one pass. We already have the components: `token_gate_adapter` speculative draft tables, `bench/spec_decode.md`, gguf bigram speculation (plan 15). vLLM V1 ships this as standard (`--speculative-config`).

**For us:** already on the roadmap; nothing new to distill except the *async scheduling* detail — V1 rejects/accepts drafts inside the same per-step scheduler loop, so spec decode composes with continuous batching rather than being a separate path. Keep that in mind when wiring plan 15 into the engine.

### 6. Process architecture (API server / engine core / workers)

vLLM splits HTTP from engine core via ZMQ because engine code is not asyncio-safe — the same shape as our non-Send MLX thread behind an mpsc channel. Their multi-process/DP/TP machinery is irrelevant at our scale.

**For us:** validated, skip the rest. One divergence worth noting: vLLM's API server **streams** results to clients (token-level); our oneshot `GenReply` is the last remaining batch-style surface. Streaming via an `mpsc` per job instead of oneshot is a small change (engine already has the per-token loop) and unblocks the backend's `/v1/chat/completions` SSE passthrough.

### 7. Uniform config object (`VllmConfig`) + uniform model ctor

Every layer of vLLM takes one config object; every model constructor has the same signature, which lets the runner build 50+ model types and compose VLMs without per-type glue.

**For us:** `mlx-rs::engine::Model::supported(&path)` + `Model::load` is already the uniform interface across `qwen3_5` hybrid and gemma dense — keep it that way. When adding a third architecture, resist per-model params on `load`; extend the config struct instead (matches the "no hardcoded values / prefer enums" workspace rule).

### 8. Weight sharding/quantization at load, not after

vLLM shards + quantizes *during* model init to avoid materializing full weights. Our 4-bit checkpoints are already quantized on disk and `Model::load` dequantizes lazily per layer — same principle, already correct. Nothing to do.

## Priority for local_model

| # | Technique | Verdict | Effort |
|---|-----------|---------|--------|
| 1 | Continuous batching (round-robin first) | **Adopt** — kills head-of-line blocking | M |
| 2 | Prefix KV caching (agent system prompts) | **Adopt** — biggest latency win for agents | M–L |
| 3 | Streaming replies (mpsc not oneshot) | **Adopt** | S |
| 4 | KV block accounting in stats | Adopt ( groundwork for 1–2) | S |
| 5 | Spec decode integration (compose with scheduler) | Already planned (15) — keep composable | M |
| 6 | Full PagedAttention block table | Defer until multi-sequence batching lands | L |
| 7 | Chunked prefill | After continuous batching | S |
| 8 | Multi-process/TP/DP, VllmConfig-style config | Skip (scale mismatch) | — |

## Suggested order of implementation

1. Engine stats: KV bytes per sequence (makes everything after measurable).
2. Streaming: `Job.reply` oneshot → `tokio::sync::mpsc`, SSE endpoint.
3. Continuous batching loop in `serve_jobs` with `MAX_ACTIVE_SEQUENCES`.
4. Prompt-prefix KV reuse keyed on chat-template-wrapped prefix hash.
5. Fold plan 15 (bigram spec decode) into the step scheduler.

Benchmark each step against `bench/roofline.md` baselines; report decode tps and queue wait time (p50/p95) in `bench/`.

## Sources

- https://docs.vllm.ai/en/latest/design/arch_overview.html
- https://docs.vllm.ai/en/latest/design/paged_attention.html
- https://blog.vllm.ai/2023/06/20/vllm.html
