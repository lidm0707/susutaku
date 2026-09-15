---
name: susutaku-project
description: How to work in the susutaku workspace — build/run commands, crate map, docker stacks, e2e tests, task/pipeline/chat APIs, and where plans and benchmarks go. Use when doing any task in this repo.
---

# Working in susutaku

Read `AGENTS.md` at the repo root first — it is the source of truth for the
tree layout and rules. This skill is the practical quickstart.

## Golden rules

- Pure Rust only; no shell-out glue where a crate exists.
- After every task: `cargo check --workspace && cargo clippy --workspace` must be clean.
- Tests live in each crate's `tests/` dir (integration, public API only).
- Plans go in `.plans/` numbered 00–99 (100+ used historically; pick the next
  free number). Update the plan file's status when a task finishes.
- No `#[allow(dead_code)]` — use `todo!()`/`unimplemented!()` instead.
- Constants/enums over magic numbers; prefer enum + `match` over if-chains.
- sqlx macros compile against the live Postgres — keep it up when touching
  `task-rs`/`backend`.

## Build & run

```sh
cargo check --workspace          # must pass
cargo clippy --workspace         # must be clean
make run                         # backend + web (see Makefile for targets)
```

Postgres (needed for task-rs/backend sqlx macros and the board):

```sh
docker compose -f docker/compose/base.yml up -d postgres
# postgres://susutaku:susutaku@localhost:5434/susutaku  (host port 5434!)
```

## Where things live (only the frequently-needed bits)

| Task | Go to |
|---|---|
| Chat tool loop, board tools, agent use case | `backend/src/app/chat.rs`, `backend/src/domain/service/` |
| Board ops exposed to the chat agent | `backend/src/domain/valueobject/board.rs` + `backend/src/app/board.rs`; tool lines parsed in `backend/src/domain/service/tool_call.rs`; prompt text in `backend/src/domain/service/prompt.rs` |
| Card pipeline execution | `backend/src/app/pipeline_run/` (run_card_pipeline, stages in `nodes.rs`) |
| Cron scheduler for cards | `backend/src/app/schedule_work.rs` (card.cron, 5-field UTC) |
| HTTP API | `backend/src/api.rs` (axum; task routes, chat routes, cronjobs) |
| Pipeline graph/stage engine | `crates/piplines` (PipelineSpec, validate_draft) |
| Task model + Postgres store | `crates/task-rs` (`query_as!`, Postgres on 5434) |
| Agent sandbox | `crates/core-agent/src/sandbox/{macos,linux,windows}.rs` |
| Web UI (React TS, vite) | `web_ui/src/` — overlays: SlideOver for heavy content, Modal for light (in `src/ui/Overlay.tsx`) |
| Inference | remote always: `RemoteModel` from `SUSUTAKU_LOCAL_MODEL_URL` (default 127.0.0.1:8992); MLX never runs in the backend container |

## E2E (playwright)

Pipeline editor walkthrough: `playwright/tests/pipeline-run.spec.ts`.
Isolated stack (artifacts land in `check_pipe/`):

```sh
docker compose -f docker/compose/playwright-pipe.yml up --build --exit-code-from playwright
```

Gotchas:
- compose `build:` MUST set `target: backend-runtime` (last stage is hub-runtime).
- e2e admin creds are auto-seeded owner/owner + password rotation
  (`E2E_ADMIN_NEW_PASSWORD`).
- gate services with healthchecks, not plain `depends_on`.
- compose relative paths resolve against `docker/compose/`, so repo-root
  mounts need `../../`.

## Benchmarks / design summaries

Write benchmark and coverage summaries to `bench/` (e.g. design renders from
`crates/design_render`).

## Chat → card → pipeline → routine flow (as built)

1. Model emits `TOOL: CARD_FIND <words>` → ranked search over cards.
2. Found → reuse id; not found → `CARD_CREATE`.
3. `PIPELINE_CREATE` (+ spec JSON) then `CARD_LINK`.
4. `CARD_ROUTINE <card_id> <cron>` for recurring runs (host scheduler),
   `CARD_ROUTINE_CLEAR` to stop, `CARD_RUN <card_id>` to run once now.
Permission kinds: board/card/pipeline/routine — checked per agent allow-list.
