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
│   ├── core-agent/            ← agent state, sandbox (sandbox/{macos,linux,windows}.rs), web search
│   ├── manager-rs/            ← manager process: agent sandboxes, run/logs API
│   ├── local_model/           ← standalone model server (MLX on Metal, hub, HTTP API)
│   │                            — owns model size/quant policy (see its AGENTS.md)
│   ├── hf_loader/             ← model dir scanning + loadability filter (policy: local_model/AGENTS.md)
│   ├── mlx-rs/                ← MLX inference backend
│   ├── gguf-rs/               ← GGUF model file parsing
│   ├── kanban-rs/             ← kanban board + Postgres store (query_as!)
│   │                            workspace → project → card; card agent state
│   ├── piplines/              ← pipeline graph/stage engine
│   ├── prompt-sys/            ← prompt builder + sections
│   ├── proto-rs/              ← hub/client protocol (codec, client, server)
│   ├── queue-rs/              ← bounded/unbounded queues
│   ├── pdf-rs/                ← PDF parsing
│   ├── work/                  ← services on top (cron worker, model service, pdf2csv)
│   ├── design_render/         ← bin: paints every UI page → bench/design/ PNG + boxes.json
│   ├── agent_3th_cli/         ← claude_cli · codex_cli (CLI integrations)
│   ├── cloud_model_api/       ← zai_api · ai_interface_layer (cloud model HTTP APIs)
├── web_ui/                    ← React TS UI (vite): pages/, components/, ui/, api/
├── playwright/                ← e2e suite (tests/, fixtures, mock-model server)
├── docker/                    ← grouped by purpose:
│   ├── compose/               ← base · demo · deploy · sandbox · playwright*
│   ├── backend/               ← Dockerfile.backend · entrypoint · Dockerfile.client
│   ├── web/                   ← Dockerfile.web · nginx.conf
│   ├── mock/                  ← Dockerfile.mock-model
│   └── test/                  ← Dockerfile.playwright
├── models/                    ← local model dirs (see names_models.md)
├── .plans/                    ← numbered plan files (00–99) — write one per task
├── bench/                     ← benchmark/design summaries (coverage, design renders)
├── docs/                      ← workflow docs (attachments, playwright, docker)
├── attachments/               ← uploaded file storage
├── piplines/ input/ output/   ← pipeline assets
```

## Layout

- `backend/` — HTTP backend (agent API, sandbox adapter, auth, settings)
- `crates/core-agent` — agent state, sandbox (macOS/Linux/Windows, `sandbox/{macos,linux,windows}.rs`), web search
- `crates/hf_loader` — model dir scanning + loadability filter (policy: `crates/local_model/AGENTS.md`)
- `crates/mlx-rs` — MLX inference backend
- `crates/local_model` — standalone model server (MLX on Metal, hub, HTTP API).
  Owns the model size/quantization policy — see `crates/local_model/AGENTS.md`.
- `crates/pdf-rs` — PDF parsing
- `crates/kanban-rs` — kanban board model + Postgres store (`query_as!`); hierarchy
  workspace → project → task (card); cards carry per-card agent state (`agent_name`, `agent_state` JSON)
- `crates/gguf-rs` — GGUF model file parsing
- `crates/agent_3th_cli/` — third-party CLI integrations (`claude_cli`, `codex_cli`)
- `crates/cloud_model_api/` — cloud model HTTP APIs (`zai_api`, `ai_interface_layer`)
- `crates/work` — applications/services built on the crates above
- `attachments/` — file attachment storage (see `docs/attachments.md`)
- `piplines/`, `input/`, `output/`, `web_ui/` — pipeline and UI assets (`web_ui` includes a Kanban board page backed by `crates/kanban-rs` + Postgres)

## Backend in Docker (standalone)

- The backend can run alone in a container (`docker/backend/Dockerfile.backend`,
  `debian:stable-slim`) and execute agents in-container: on Linux it uses the
  rootless sandbox in `crates/core-agent/src/sandbox/linux.rs` (userns +
  mount ns + chroot jail + seccomp deny-list). No macOS/Metal dependency in
  the agent path. Verified end-to-end: spawn → run (real stdout, jailed fs,
  workspace persistence across runs) → finish, all inside a private
  compose stack (`docker/compose/sandbox.yml` — postgres + backend,
  no published ports; test via `docker compose exec` + curl).
- Required compose flags — the sandbox refuses to run unsandboxed, so
  namespace creation must be allowed:
  `security_opt: [seccomp=unconfined, apparmor=unconfined]`.
- Inference is always remote: `main.rs` builds `RemoteModel` from
  `SUSUTAKU_LOCAL_MODEL_URL` (default `127.0.0.1:8992`). In a container set it
  to the host model server (`http://host.docker.internal:8992`) — MLX itself
  never runs inside the backend container.
- `DATABASE_URL` must point at the postgres service (not `localhost:5434`).
- Sandbox network is loopback-only (`linux.rs` limitations): agent commands
  inside the sandbox have no internet; `claude`/`codex` CLIs must be baked
  into the image to be usable and run outside the sandbox.

# Podman sandbox

- Nested rootless podman inside the backend container (deploy stack):
  required compose flags, storage/cgroup fixes, image lifecycle, and a
  verification checklist — see `docs/podman-sandbox.md`.

## Kanban Postgres

- Postgres runs via `docker/compose/base.yml` (`postgres` service, host port **5434** — 5432/5433 are taken by other local containers).
- Connection: `postgres://susutaku:susutaku@localhost:5434/susutaku` (override with `DATABASE_URL`).
- sqlx macros compile against the live DB — keep the container up when running `cargo check` on `kanban-rs`/`backend`.
- API: `/api/workspaces` (GET/POST), `/api/workspaces/{id}` (DELETE),
  `/api/workspaces/{id}/projects` (GET/POST), `/api/projects/{id}` (DELETE).
- Tasks: `/api/kanban/cards?project_id=` (GET/POST), `/api/kanban/cards/{id}` (DELETE),
  `/api/kanban/cards/{id}/move` (POST), `/api/kanban/cards/{id}/agent` (GET/PUT).
  A task (card) belongs to exactly one project; deleting a workspace cascades
  to its projects and tasks.

## User auth (argon2)

- `kanban-rs/src/user.rs`: argon2 password hashing, ranked `Role` enum —
  `owner > super_admin > admin > editor > viewer`.
- Tables: `users(username unique, password_hash, role)`, `auth_sessions(token, user_id)`.
- Bearer-token auth: `POST /api/auth/login`, `POST /api/auth/logout`,
  `GET /api/auth/bootstrap`, `GET/POST /api/auth/users`.
- Bootstrap: with zero users, an unauthenticated `POST /api/auth/users` creates
  the first user, forced to role `owner`.
- Guards: any role reads kanban; editor+ mutates cards/agent state;
  admin+ manages users. Passwords: min 8 chars, never returned.

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
