# Thread environments: per-thread sandboxes, card binding and merge

Design for isolated coding environments per chat thread. Phase 1 is
implemented; phases 2–4 are designed here and planned in
`.plans/119-thread-env-card-merge.md`.

## Why

Before: every chat thread shared one global sandbox, so two threads coding at
the same time overwrote each other's files, and nothing a thread did showed up
on the review page (which inspects per-agent manager slots only).

Now: **one thread = one sandbox = one work tree** — isolated, inspectable,
deletable when done.

## Phase 1 — thread environments (implemented)

- First coding tool use in a thread (SHELL / coding / LSP) spawns a podman
  sandbox rooted at `work/thread-envs/<thread_id>` (host side, one directory
  per thread). Same thread reuses it; different threads are fully isolated.
- Threads without an id, or when the environment cannot be created, keep
  using the shared work tree — old behavior unchanged.
- Artifacts a turn produces (files the agent wrote) are attached to the
  target card from that thread's own work tree.

### Lifecycle / deletion policy (user decision: delete on use, 24h only as safety net)

| Trigger | When the environment is deleted |
|---|---|
| Delete button on the review environment list | immediately (confirm first) |
| Successful merge into the project main work tree (phase 4) | right after the merge |
| Bound card closed/completed (phase 2) | when the card closes |
| 24h idle without any tool use | safety-net sweep |

Implementation notes:

- `AgentSandbox::for_work_tree(dir)` wraps `Sandbox::new_in(...).keep_on_drop()`
  — a plain `Sandbox` **deletes its root on drop**; `keep_on_drop()` hands the
  directory lifecycle to the environment manager instead.
- `ThreadEnvManager::sweep` runs on every resolve: drops map entries idle
  beyond 24h (deletes their directory) and removes orphan directories left by
  a previous backend process (mtime-based). No background task needed.

## Phase 2 — card binding (planned)

- A thread may carry a `card_id` (UI: drop a card into the thread, or a card
  menu action "open thread"); the env row stores it.
- Every turn's artifacts attach to that card automatically (the mechanism
  exists: `ResourceService` + `card_id` on the turn — the env makes it the
  default).
- The card detail view shows the linked environment and a diff summary.
- Closing/completing the card purges the environment.

## Phase 3 — review per environment (planned)

- The review page lists **environments**: manager agent slots (as today) plus
  thread envs.
- Each environment renders the same diff panel (files, +adds/−dels, click for
  the patch modal) and the commit / push / PR actions.
- Each environment has an "adjust again" chat input that posts into the bound
  thread — the agent keeps working in exactly that work tree.

## Phase 4 — merge to local main + conflicts (planned)

- The backend keeps one managed clone per project at
  `work/projects/<project_id>` (the "local main work tree").
- Merge flow: commit the env's work → merge into the project's main work
  tree (git2 in `git-rs`, pure Rust — no `git` binary).
- **On success** the environment is purged.
- **On conflict** the merge aborts with the list of conflicted files and:
  - the review page shows a conflict alert + badge;
  - a chat message lands in the bound thread with the conflict files;
  - the user chooses: type in chat ("fix the merge") and the agent edits /
    commits / the merge is retried — or resolve manually in the editor and
    mark resolved, then retry.

## Code map (implemented parts)

| Piece | File |
|---|---|
| Port | `backend/src/port/outbound/thread_envs.rs` (`trait ThreadEnvs`) |
| Manager | `backend/src/infra/thread_env.rs` (`ThreadEnvManager`, TTL + sweep) |
| Non-destructive sandbox | `crates/core-agent/src/podman/sandbox.rs` (`keep_on_drop`), `backend/src/infra/podman.rs` (`for_work_tree`) |
| Turn routing | `backend/src/app/chat.rs` (`turn_runner`, `shell_blocking_on`, runner-parameterized artifact scan/persist) |
| Wiring | `backend/src/main.rs`, `backend/src/api.rs` (`ZaiChatDeps.thread_envs`) |
| Tests | `backend/tests/thread_env.rs` |
