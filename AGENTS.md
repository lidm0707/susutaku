# AGENTS.md

## Project tree

```
susutaku/
├── AGENTS.md                  ← this file (workspace rules; per-crate AGENTS.md override)
├── Cargo.toml                 ← workspace: all crates below
├── Makefile                   ← make run/up/deploy/e2e/cover … (compose + host targets)
├── backend/                   ← HTTP backend (axum): agent API, sandbox adapter,
│    │                            auth, settings, quota board
│    └── src/{api.rs, app/, domain/, infra/, port/}
├── crates/
│   ├── core-agent/            ← agent state, podman sandbox (podman/{sandbox,runner,image,git_in_sandbox}.rs),
│   │                            sandbox contract (sandbox_abstract_layer.rs), toolcalls, web search
│   ├── manager-rs/            ← manager process: per-agent/per-task slots, work trees, run/logs API
│   ├── local_model/           ← standalone model server (MLX on Metal, hub, HTTP API)
│   │                            — owns model size/quant policy (see its AGENTS.md)
│   ├── hf_loader/             ← model dir scanning + loadability filter (policy: local_model/AGENTS.md)
│   ├── mlx-rs/                ← MLX inference backend
│   ├── gguf-rs/               ← GGUF model file parsing
│   ├── task-rs/             ← task board + Postgres store (query_as!)
│   │                            workspace → project → card; card agent state, image, cron, run records
│   ├── piplines/              ← pipeline graph/stage engine (in workspace; no longer wired to the backend)
│   ├── prompt-sys/            ← prompt builder + sections
│   ├── proto-rs/              ← hub/client protocol (codec, client, server)
│   ├── queue-rs/              ← bounded/unbounded queues
│   ├── pdf-rs/                ← PDF parsing
│   ├── work/                  ← services on top (cron worker, model service, pdf2csv)
│   ├── design_render/         ← bin: paints every UI page → bench/design/ PNG + boxes.json
│   ├── agent_3th_cli/         ← claude_cli · codex_cli (CLI integrations)
│   ├── cloud_model_api/       ← zai_api · ai_interface_layer (cloud model HTTP APIs)
│   ├── git-rs/                ← git control over a work tree via git2 (commit, patch, dirty state; host-side only)
│   ├── lsp-rs/                ← LSP client/codec + tools
│   ├── knowledge_graph/       ← entity/triple/graph (knowledge graph + entity embedder)
│   ├── latenspace/            ← latent context store (token-budgeted context entries)
│   ├── token_gate_adapter/    ← TGA: tokenizer backend + speculative draft tables (HF tokenizers / katgpt BPE)
│   ├── modelless/             ← engine-free language kit (English verb corpus + katgpt BPE)
│   ├── research-rs/           ← research/distill utilities
│   ├── prompt_bench/          ← bin: prompt benchmarking
│   ├── codex-usage-rs/        ← codex usage analytics (rollouts, scheduler, snapshots, store)
│   ├── wgpu-rs/               ← GPU/wgpu cdylib+rlib backend
│   ├── solana_wallet/         ← browser wallet keypair + RPC helpers (wasm-ready, gloo)
│   ├── math/                  ← pure-Rust math: geomath, linalg, stats, complex, vec
│   ├── physic/ bio/ chemi/    ← domain simulation crates
│   ├── quatum/                ← qubit state vectors + gates
│   ├── plan/                  ← plan/task model (Plan, Task, Priority, Status)
│   ├── text_ide/              ← text buffer, cursor, search, undo/redo editor
├── web_ui/                    ← React TS UI (vite): pages/, components/, ui/, api/
├── playwright/                ← e2e suite (tests/, fixtures, mock-model server)
│                                chat/board/review/auth/ux specs; artifacts → screenshots/ + check_pipe/
├── docker/                    ← grouped by purpose:
│   ├── compose/               ← base · demo · deploy · sandbox · playwright*
│   │                            playwright-pipe.yml = isolated pipeline e2e stack
│   ├── backend/               ← Dockerfile.backend · entrypoint · Dockerfile.client
│   ├── web/                   ← Dockerfile.web · nginx.conf
│   ├── mock/                  ← Dockerfile.mock-model
│   └── test/                  ← Dockerfile.playwright
├── models/                    ← local model dirs (see names_models.md)
├── .plans/                    ← numbered plan files (00–99) — write one per task
├── check_pipe/                ← artifacts of the playwright-pipe e2e stack
├── check_real/                ← artifacts of real-run checks
├── assets/                    ← static assets
├── log/                       ← runtime logs
├── .sqlx/                     ← sqlx query metadata (offline compile)
├── bench/                     ← benchmark/design summaries (coverage, design renders)
├── docs/                      ← workflow docs (attachments, playwright, docker,
│                                review flow, work tree, podman sandbox, git sandbox)
├── attachments/               ← uploaded file storage
├── piplines/ input/ output/   ← pipeline assets
```

## Layout

- `backend/` — HTTP backend (agent API, sandbox adapter, auth, settings)
- `crates/core-agent` — agent state, sandbox. The platform contract lives in
  `sandbox_abstract_layer.rs`; the shipping backend is the **rootless podman
  sandbox** (`podman/`: sandbox, runner, limits, image mgmt, git-in-sandbox).
  Toolcalls (SHELL/GIT/LSP/coding/…) live in `toolcall/`, web search included.
- `crates/hf_loader` — model dir scanning + loadability filter (policy: `crates/local_model/AGENTS.md`)
- `crates/mlx-rs` — MLX inference backend
- `crates/local_model` — standalone model server (MLX on Metal, hub, HTTP API).
  Owns the model size/quantization policy — see `crates/local_model/AGENTS.md`.
- `crates/pdf-rs` — PDF parsing
- `crates/task-rs` — task board model + Postgres store (`query_as!`); hierarchy
  workspace → project → task (card); one entity per piece of work — a Board
  Card IS a Task. Cards carry agent state (`agent_name`, `agent_state` JSON),
  an optional sandbox `image`, and a canonical `TaskStatus`
  (todo/in_progress/review/conflict/done/failed = board column, transitions
  validated server-side); runs are recorded (`run_records`,
  `GET /api/task/cards/{id}/runs`). Routines are a **separate** entity
  (`routines` + `routine_runs` tables, same store) — recurring automation,
  never a card.
- `crates/gguf-rs` — GGUF model file parsing
- `crates/agent_3th_cli/` — third-party CLI integrations (`claude_cli`, `codex_cli`)
- `crates/cloud_model_api/` — cloud model HTTP APIs (`zai_api`, `ai_interface_layer`)
- `crates/work` — applications/services built on the crates above
- `crates/math`, `physic`, `bio`, `chemi`, `quatum`, `plan`, `text_ide` —
  domain/engine-free libraries (math, simulations, qubit gates, plan model, text editor kit)
- `crates/git-rs` — git work-tree control via git2 (commit-all, patch text, dirty state); host-side only
- `crates/lsp-rs` — LSP client, codec, tool bindings
- `crates/knowledge_graph` — entity/triple/graph model with entity embedder
- `crates/latenspace` — latent context: token-budgeted `Context` entries
- `crates/token_gate_adapter` — token gate adapter: unified encode/decode trait over HF `tokenizers` and katgpt BPE, plus speculative draft tables
- `crates/modelless` — engine-free language kit: English verb corpus + katgpt BPE engine
- `crates/research-rs` — research/distill utilities
- `crates/prompt_bench` — prompt benchmarking binary
- `crates/codex-usage-rs` — codex usage analytics: rollout parsing, scheduler, snapshots, store
- `crates/wgpu-rs` — wgpu GPU backend (cdylib + rlib)
- `crates/solana_wallet` — browser wallet keypair (ed25519-dalek) + RPC helpers,
  wasm-ready via gloo; localStorage persistence
- `attachments/` — file attachment storage (see `docs/attachments.md`)
- `piplines/`, `input/`, `output/`, `web_ui/` — pipeline and UI assets (`web_ui` includes a Task board page backed by `crates/task-rs` + Postgres)

## Backend in Docker (standalone)

- The backend can run alone in a container (`docker/backend/Dockerfile.backend`,
  `debian:stable-slim`) and execute agents in-container: agent commands run in
  the **rootless podman sandbox** (`crates/core-agent/src/podman/`) — nested
  podman with the sandbox image built at container start. No macOS/Metal
  dependency in the agent path. Private compose stack:
  `docker/compose/sandbox.yml` (postgres + backend, no published ports; test
  via `docker compose exec` + curl).
- Required compose flags — nested podman needs namespace/FUSE/cgroup
  delegation: `security_opt: [seccomp=unconfined, apparmor=unconfined]`,
  `privileged: true`, `/dev/fuse` (full story: `docs/podman-sandbox.md`).
- Inference is always remote: `main.rs` builds `RemoteModel` from
  `SUSUTAKU_LOCAL_MODEL_URL` (default `127.0.0.1:8992`). In a container set it
  to the host model server (`http://host.docker.internal:8992`) — MLX itself
  never runs inside the backend container.
- `DATABASE_URL` must point at the postgres service (not `localhost:5434`).
- Agent sandbox network defaults to `--network=none`: agent commands have no
  internet unless a run explicitly enables it (e.g. in-sandbox git push/pr);
  `claude`/`codex` CLIs must be baked into the image to be usable.

# Podman sandbox

- Nested rootless podman inside the backend container (deploy stack):
  required compose flags, storage/cgroup fixes, image lifecycle, and a
  verification checklist — see `docs/podman-sandbox.md`.

## Playwright e2e

- Specs live in `playwright/tests/` (auth, task board, card detail/run,
  chat mock/real/modal/dock, review, settings, graph, worker, walkthrough,
  ux-snapshots). The old pipeline-editor spec is gone — cards run their
  assigned agent directly (`backend/src/app/card_run.rs`), no pipeline nodes.
- Fully-containerized stack: `docker/compose/playwright-backend.yml`
  (postgres + mock-model + backend + web + playwright, no published ports);
  host-backend variant: `docker/compose/playwright.yml`. Details:
  `docs/playwright-workflow.md`, `docs/playwright-backend-workflow.md`.
- `Dockerfile.backend` is multi-stage and its LAST stage is `hub-runtime` —
  compose `build:` MUST set `target: backend-runtime` or the container
  silently runs the hub stub instead of the backend.
- The backend auto-seeds `owner`/`owner` (must_change_password) on a fresh DB,
  so e2e admin creds are owner/owner + rotation (`E2E_ADMIN_NEW_PASSWORD`),
  not a separate e2e-admin user.
- Compose relative paths resolve against the compose file's dir
  (`docker/compose/`), not the CWD — repo-root mounts need `../../playwright/...`.
- Waiting on backend readiness: gate dependent services with a healthcheck
  (`depends_on: condition: service_healthy`); a plain `depends_on` starts the
  playwright runner while nginx still 502s.

## Task Postgres

- Postgres runs via `docker/compose/base.yml` (`postgres` service, host port **5434** — 5432/5433 are taken by other local containers).
- Connection: `postgres://susutaku:susutaku@localhost:5434/susutaku` (override with `DATABASE_URL`).
- sqlx macros compile against the live DB — keep the container up when running `cargo check` on `task-rs`/`backend`.
- API: `/api/workspaces` (GET/POST), `/api/workspaces/{id}` (DELETE),
  `/api/workspaces/{id}/projects` (GET/POST), `/api/projects/{id}` (DELETE).
- Tasks: `/api/tasks?project_id=` (GET) and `POST /api/tasks` (create; status
  defaults todo) are the canonical surface; `/api/task/cards*` remain as
  backward-compatible aliases (`/api/task/cards/{id}` DELETE/PUT,
  `/{id}/move` POST, `/{id}/agent` GET/PUT, `/{id}/schedule` PUT — legacy,
  no longer scheduled, `/{id}/image` PUT, `/{id}/run` POST — runs the card's
  assigned agent, `/{id}/runs` GET history, `/{id}/resources` GET,
  `/{id}/comments` GET/POST).
- Task status: `PATCH /api/tasks/{id}/status` — the backend validates the
  transition (`TaskStatus::transition_allowed` in `crates/task-rs`); the
  frontend is never the authority. Statuses: todo, in_progress, review,
  conflict, done, failed.
- Routines (owner-handled, separate from tasks): `GET/POST /api/routines`,
  `PUT/DELETE /api/routines/{id}`, `POST /api/routines/{id}/run` (owner role
  only), `GET /api/routines/{id}/runs` (any role). The scheduler scans
  enabled routines only; card cron is no longer scheduled.
  A task (card) belongs to exactly one project; deleting a workspace cascades
  to its projects and tasks.
- Agent outputs (durable review artifacts): `/api/agent-outputs`,
  `/api/agent-outputs/{id}` (GET, DELETE), `{id}/status` approve/reject —
  see `docs/review-flow.md`. On the Review page, opening a PR auto-finishes
  the agent: work tree torn down, output stored (PR is the end state).
- Card runs: `backend/src/app/card_run.rs` — run the card's assigned agent
  (model inference + its tools); there is no pipeline stage engine in the
  backend anymore (`crates/piplines` remains but is unwired).

## User auth (argon2)

- `task-rs/src/user.rs`: argon2 password hashing, ranked `Role` enum —
  `owner > super_admin > admin > editor > viewer`.
- Tables: `users(username unique, password_hash, role)`, `auth_sessions(token, user_id)`.
- Bearer-token auth: `POST /api/auth/login`, `POST /api/auth/logout`,
  `GET /api/auth/bootstrap`, `GET/POST /api/auth/users`.
- Bootstrap: with zero users, an unauthenticated `POST /api/auth/users` creates
  the first user, forced to role `owner`.
- Guards: any role reads task; editor+ mutates cards/agent state;
  admin+ manages users; **owner only** manages/runs routines. Passwords:
  min 8 chars, never returned.

## Web UI conventions (`web_ui/`)

- Overlay choice: lots of information (detail views, long lists, multi-section
  content) → `SlideOver`; light content (short forms, confirmations, single
  input) → `Modal` / `PromptModal`. Components live in `src/ui/Overlay.tsx`.
- Semantic layout: use landmarks/elements for their meaning (`main`, `header`,
  `nav`, `section`, `form`, `article`) and ARIA (`role="dialog"`,
  `aria-modal`, labels) — not div soup.
- Minimal design: no decorative chrome, few colors, small icon set
  (`lucide-react`), sparse borders/shadows.

## Rules

- Use **pure Rust** — no Python scripts, no shell-out glue where a Rust crate exists; Rust-only dependencies, no C/C++ build wrappers unless unavoidable
- Every finished task: `cargo check && cargo clippy`
- Tests live in each crate's `tests/` dir (integration tests, public API only) — no inline `#[cfg(test)] mod tests` in `src/`
- New plans go in `./.plans`, numbered 00–99
- Prefer enums over hard-coded values; constants for all magic numbers
- Prefer enum + `match` over scattered if-chains: dispatch on an enum of
  variants (each variant renders/decides for itself) instead of chains of
  `if x { .. } if y { .. }` on flags or strings — see `prompt.rs` `Section`
