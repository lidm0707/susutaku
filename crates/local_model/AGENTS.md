# local_model — AGENTS.md (crate context)

## What this crate is

`local_model` is the standalone model server binary (`cargo run -p local_model`):
it owns MLX model loading and inference on Apple Silicon (Metal), plus a hub
that multiplexes jobs over TCP and HTTP. The backend never loads models
itself — it talks to this server remotely.

## Model policy (moved here from the root AGENTS.md)

These rules apply to everything that loads or filters local models
(`local_model`, `hf_loader`, `mlx-rs`):

- Only load MLX models **larger than 30 GiB** on disk.
- Only use **4-bit (Q4) quantized** checkpoints (e.g. `-4bit` / `4bit`
  variants) — the engine's `loadable` filter enforces the 4-bit half.
- First supported model: `qwen3.8 27b mlx`
  (`mlx-community/Qwen3.8-27B-4bit`, qwen3_5 hybrid).
- Second model: `gemma4 26b a4b it mlx`
  (`mlx-community/gemma-4-26b-a4b-it-4bit`, dense) — engine port complete,
  parity-verified vs mlx-lm.
- Exception: `gemma4 e4b mlx` (`mlx-community/gemma-4-e4b-it-4bit`, ~4.8 GB)
  is supported despite being under the size floor (bring-up exception,
  plan 12).
- Model dirs live in `models/` (inventory: `models/names_models.md`).
- Model selection at runtime goes through the backend's `POST
  /api/models/select`; discovery comes from `hf_loader::loadable_models`.
  Never hardcode model paths — always scan `models/`.

## Layout

- `src/engine.rs` — MLX model pool: spawns/loads models, enforces the 4-bit
  loadability filter, runs generation
- `src/hub.rs` — job hub: multiplexes inference + command jobs across pools
- `src/api.rs` — HTTP API (model catalog, select, inference) and app state
- `src/ports.rs` — TCP/HTTP port constants (`*_ENV` vars override)
- `src/lib.rs` — shared exports

## Protocol

- The backend's `RemoteModel` (backend/src/infra/model_client.rs) is the
  client. It speaks **OpenAI-compatible first** (`GET /v1/models`,
  `POST /v1/chat/completions`) and falls back to this crate's native
  protocol (`GET /api/models`, `POST /api/inference`) when `/v1/models`
  is absent (see plan 43).
- The endpoint is settable from the web UI (Settings → ai providers →
  local model); precedence: `setting.json local.endpoint` >
  `SUSUTAKU_LOCAL_MODEL_URL` env > `http://127.0.0.1:8992`.

## Rules

Inherit the workspace rules from the root `AGENTS.md` (pure Rust,
`cargo check && cargo clippy`, integration tests in `tests/`, plans in
`./.plans`). This crate additionally owns the model size/quantization
policy above — do not duplicate that policy in other crates' docs; link
here instead.
