# susutaku

<div align="center">
  <img src="assets/susutaku_jibi.png" alt="susutaku mascot" width="360" />
</div>

**susutaku is a harness agent**: it plans work, automates executing that plan,
and scales execution across machines through a manager process — powered by an
improved local model server.

```
plan → pipeline of stages → manager process spawns sandboxed agents → results + state
                ↑
   local model server (MLX / GGUF hub)
```

## What it does

1. **Plan** — work is captured as plans (`.plans`, numbered 00–99) and tasks on
   a Kanban board (workspace → project → card), each card carrying per-card
   agent state.
2. **Automate the plan** — `piplines` wires named stages into a sequential
   graph; a payload flows stage to stage until the plan's steps have run.
3. **Scale across machines** — the **manager process** (`manager-rs`) owns one
   sandboxed work tree per agent, spawns agent sandboxes on demand, runs the
   agent's commands inside them, and on task finish returns the result plus the
   sandbox state (cwd + transcript) before teardown. Stale work trees from
   crashed runs are reclaimed automatically.
4. **Local model server** — `local_model` serves inference over HTTP + a TCP
   hub: a pool of dedicated inference threads (MLX and GGUF engines), model
   switching at runtime, and a hub-only mode for distributed deployments.

## Architecture

| Path | Description |
|---|---|
| `backend/` | HTTP backend: harness/agent API, sandbox adapter, auth, settings |
| `crates/manager-rs` | Manager process: per-agent/per-task sandboxed work trees, spawn/run/finish + push/PR, scaling |
| `crates/core-agent` | Agent state, rootless podman sandbox (`podman/`), toolcalls, web search |
| `crates/local_model` | **Local model server**: HTTP API + TCP hub, engine pool (MLX/GGUF) |
| `crates/mlx-rs` | MLX inference backend |
| `crates/hf_loader` | Model dir scanning + loadability filter (size/quant policy: see `local_model` AGENTS.md) |
| `crates/task-rs` | Task board model (pure data — SQL lives in `backend/src/infra/postgres/`) |
| `crates/pdf-rs` | PDF parsing |
| `crates/gguf-rs` | GGUF model file parsing |
| `crates/agent_3th_cli/` | `claude_cli`, `codex_cli` |
| `crates/cloud_model_api/` | `zai_api`, `ai_interface_layer` |
| `crates/work` | Applications/services built on the crates above |
| `crates/queue-rs` | Queueing |
| `crates/prompt-sys` | Prompt handling |
| `crates/math` · `physic` · `bio` · `chemi` · `quatum` | Engine-free domain libraries (math, simulations, qubit gates) |
| `crates/plan` · `text_ide` · `git-rs` · `lsp-rs` | Plan model, text editor kit, git2 work-tree control, LSP client/tools |
| `web_ui/` | Web UI, including the Kanban board page |
| `docker/` | Backend container image + compose stacks |
| `models/` | Local model directories |

## Local model server

`crates/local_model` (`src/main.rs`):

- **HTTP API** (`HTTP_PORT`, override with `HTTP_PORT_ENV`) — inference
  requests and model selection (`ModelSwitch`).
- **TCP hub** (`TCP_PORT`, override with `TCP_PORT_ENV`) — connects remote
  workers/backends to the model pool; multiple machines scale inference.
- **Engine pool** (`ModelPool`) — each model runs on a dedicated thread
  (owning the non-Send MLX model); jobs are submitted through a clonable
  `Engine` handle and answered via oneshot channels. GGUF models route to the
  GGUF engine, everything else to MLX.
- **Hub-only mode** — if model init fails, the hub and HTTP API still start
  (inference requests fail per-job), e.g. for a hub-only container.

Model discovery goes through `hf_loader::loadable_models` — never hardcode
model paths. Policy (`crates/hf_loader/src/lib.rs`): MLX models must be
**4-bit and at most 30 GiB** on disk; GGUF models must be **Q4** quantized;
select at runtime via `POST /api/models/select`.

### Running

```sh
cargo run -p local_model
# local-model hub listening on tcp://0.0.0.0:<tcp-port>
# local-model listening on http://0.0.0.0:<http-port>
```

The backend connects to it via `SUSUTAKU_LOCAL_MODEL_URL` (default
`127.0.0.1:8992`). In Docker, use `http://host.docker.internal:8992` — MLX
itself never runs inside the backend container.

## Sandboxed agent execution

- Every agent command runs in a **rootless podman container**
  (`crates/core-agent/src/podman/`), workspace bind-mounted, env allow-list,
  rlimits, timeout kill. The sandbox contract is `sandbox_abstract_layer.rs`;
  podman is the shipping backend on all host OSes (macOS needs
  `podman machine`).
- The backend can run agents fully inside Docker (`docker/compose/deploy.yml`,
  or the standalone `docker/compose/sandbox.yml`): nested podman requires
  `security_opt: [seccomp=unconfined, apparmor=unconfined]`, `privileged: true`
  and `/dev/fuse` — the sandbox refuses to run unsandboxed. See
  `docs/podman-sandbox.md`.
- Sandbox network defaults to `--network=none`: agent commands have no internet
  unless a run explicitly enables it (in-sandbox git push/PR); `claude`/`codex`
  CLIs must be baked into the image to be usable.

## Kanban board (plan tracking)

Postgres via `docker/compose/base.yml` (host port **5434**):

```
postgres://susutaku:susutaku@localhost:5434/susutaku
```

> `sqlx` macros compile against the live DB — keep the container up when
> running `cargo check` on `backend` (all task SQL lives in
> `backend/src/infra/postgres/`, one file per table; `crates/task-rs` is
> pure model and compiles without a DB).

API: `/api/workspaces`, `/api/workspaces/{id}/projects`,
`/api/task/cards?project_id=`, `/api/task/cards/{id}/move`,
`/api/task/cards/{id}/agent` (per-card agent name/state JSON),
`/api/task/cards/{id}/schedule` (cron), `/api/task/cards/{id}/image`,
`/api/task/cards/{id}/run` + `/runs` (run the assigned agent, view history).
Durable run output: posted to the card as a comment on finish.

Auth: argon2 password hashing, ranked roles
(`owner > super_admin > admin > editor > viewer`), bearer tokens
(`/api/auth/login`, `/api/auth/bootstrap`, `/api/auth/users`). With zero users,
an unauthenticated `POST /api/auth/users` creates the first user, forced to
role `owner`.

## Requirements

- Rust (edition 2024)
- Docker + Docker Compose (Postgres; optionally the backend container)
- macOS with Apple Silicon for local MLX inference (or a remote model server)

## Getting started

```sh
# 1. Postgres
docker compose -f docker/compose/base.yml up -d

# 2. Model server
cargo run -p local_model

# 3. Backend
cargo run -p backend
```

Backend in Docker (sandboxed agents):

```sh
docker compose -f docker/compose/sandbox.yml up -d
```

`DATABASE_URL` must point at the postgres *service*, not `localhost:5434`.

## Development rules

- Pure Rust — no Python scripts, no shell-out glue where a Rust crate exists.
- Tests live in each crate's `tests/` dir (integration tests, public API only).
- Every finished task: `cargo check && cargo clippy`.
- New plans go in `./.plans`, numbered 00–99.
- Prefer enums over hard-coded values; constants for all magic numbers.

## License

Apache-2.0 — see [LICENSE](LICENSE).
