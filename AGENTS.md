# AGENTS.md

## Model policy (harness agent)

- Only load MLX models that are **larger than 30 GiB** on disk.
- Only use **4-bit (Q4) quantized** checkpoints, e.g. `-4bit` / `4bit` variants.
- First supported model: `qwen3.8 27b mlx` (`mlx-community/Qwen3.8-27B-4bit`, qwen3_5 hybrid).
- Second model: `gemma4 26b a4b it mlx` (`mlx-community/gemma-4-26b-a4b-it-4bit`, dense) — engine port complete, parity-verified vs mlx-lm.
- Model selection at runtime via `POST /api/models/select`; discovery goes through `hf_loader::loadable_models` — never hardcode model paths.
- Exception: `gemma4 e4b mlx` (`mlx-community/gemma-4-e4b-it-4bit`, ~4.8 GB) —
  supported despite being under the size floor (bring-up exception, plan 12).
- Local model dirs live in `models/` (see `models/names_models.md`).

## Layout

- `backend/` — HTTP backend (agent API, sandbox adapter, auth, settings)
- `crates/core-agent` — agent state, sandbox (macOS/Linux/Windows, `sandbox/{macos,linux,windows}.rs`), web search
- `crates/hf_loader` — model dir scanning + loadability filter (size ≥ 30 GiB, q4, MLX)
- `crates/mlx-rs` — MLX inference backend
- `crates/pdf-rs` — PDF parsing
- `crates/kanban-rs` — kanban board model + Postgres store (`query_as!`); hierarchy
  workspace → project → task (card); cards carry per-card agent state (`agent_name`, `agent_state` JSON)
- `crates/gguf-rs` — GGUF model file parsing
- `crates/agent_3th_cli/` — third-party CLI integrations (`claude_cli`, `codex_cli`, `zai_api`, `ai_interface_layer`)
- `crates/work` — applications/services built on the crates above
- `attachments/` — file attachment storage (see `docs/attachments.md`)
- `piplines/`, `input/`, `output/`, `web_ui/` — pipeline and UI assets (`web_ui` includes a Kanban board page backed by `crates/kanban-rs` + Postgres)

## Kanban Postgres

- Postgres runs via `docker/docker-compose.yml` (`postgres` service, host port **5434** — 5432/5433 are taken by other local containers).
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
