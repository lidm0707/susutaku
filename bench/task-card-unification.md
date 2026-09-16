# Task–Card unification: board status model + transition API

Date: 2026-09-15 · Plan: `.plans/99-task-card-unification.md`

## What changed
- `crates/task-rs/src/card.rs`: canonical `TaskStatus`
  (todo/in_progress/review/conflict/done/failed) with column mapping
  (`in_progress` stored as legacy `doing` column) and `transition_allowed`.
- `backend/src/domain/service/card.rs`: `CardService::set_status` — server-side
  transition validation, reuses get/move (no new SQL, no `.sqlx` refresh).
- `backend/src/api.rs`: `GET/POST /api/tasks`, `PATCH /api/tasks/{id}/status`;
  `CardDto.status` derived from `column_id`. `/api/task/cards*` kept as aliases.
- `backend/src/app/card_run.rs`: run start → InProgress, ok → Done, fail →
  Failed via the shared enum.
- `web_ui/src/lib.ts`: `fetch_tasks`, `create_task`, `set_task_status`.
- `web_ui/src/pages/Task.tsx`: board has 6 columns (To Do, In Progress,
  Review, Conflict, Done, Failed); drag/shift use the status endpoint;
  Create Task modal (title/description/priority) posts `/api/tasks`.

## Transition table (backend is the authority)
| from \ to | todo | in_progress | review | conflict | done | failed |
|---|---|---|---|---|---|---|
| todo | – | ✓ | | | ✓ | ✓ |
| in_progress | ✓ | – | ✓ | ✓ | ✓ | ✓ |
| review | | ✓ | – | ✓ | ✓ | |
| conflict | | ✓ | ✓ | – | ✓ | |
| done | ✓ | ✓ | | | – | |
| failed | ✓ | ✓ | | | – | |

## Validation
- `cargo check --workspace` clean; `cargo clippy --workspace` clean except one
  pre-existing warning (`backend/src/infra/chat_memory.rs` variant size).
- `cargo test -p backend --test task_status_test` (new, 6 tests),
  `kanban_app_test` (5), `schedule_work_test` (7), `task-rs` suite — all pass.
- `npx tsc --noEmit` in `web_ui` — clean.

## Compatibility / migration
- No DB migration: status values are `column_id` data; `review`/`conflict`
  are new legal values of the existing TEXT column.
- Legacy column ids (`todo/doing/done/failed`) still accepted everywhere;
  `TaskStatus::parse` maps `doing` → `in_progress`.
- Cards created before this change appear in the mapped new columns.
- PR/CI state on cards is not implemented (no GitHub integration exists in
  the repo); REVIEW/CONFLICT statuses are wired for future PR webhook work.

## Follow-up: work tree teardown after PR (user request)
- Review page now auto-finishes an agent after a successful `pr` git op:
  `finish_manager_agent` purges the sandbox, deletes `work/agents/<agent>`,
  removes the manager slot, and stores the patch/transcript as an agent
  output (review happens against the stored copy).
- Git status is NOT re-fetched afterwards — it would auto-spawn a fresh
  slot and recreate the tree; the agent is dropped from the list instead.
- Teardown failure (e.g. agent homed on another client machine) keeps the
  tree alive; the PR output stays visible.
- Pre-existing e2e fix: chat-mock spec placeholder regex updated to the
  current composer copy (`/message the agent/i`).

## Follow-up: agent mentions on task comments + context snapshot
- `BoardOps::card_context(card_id)` (backend/src/port/outbound/board.rs,
  implemented in app/board.rs): compact snapshot — id, title, status
  (TaskStatus), priority, description, last 20 comments (400 chars each).
- Chat loop (app/chat.rs) prepends the snapshot when the turn carries
  `card_id` — every mention reply sees the full thread; comments are the
  persistent context store (no separate snapshot table).
- Frontend (Task.tsx + lib.ts): `chat(message, card_id, agent)` — an
  @agent comment now calls the chat turn as that named agent (persona,
  tool allow-list, thinking level apply) with card_id set, and posts the
  reply back as `<agent>: <text>` comment.
- Also: `DELETE /api/agent-outputs/{id}` + compact output rows with
  per-row delete in the agents modal.
