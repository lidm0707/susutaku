# Playwright E2E Workflow: Backend in Docker + Mock Model

Companion to `playwright-workflow.md` (which runs the backend on the host
because real MLX inference needs macOS Metal). This stack has **no host
dependency at all**: postgres, the backend, the model server and the
playwright runner all live on the internal compose network, and the model is a
deterministic mock — so chat, toolcalling, summary, the scripted card flow and
the sandboxed agent path can be tested end-to-end in CI without a GPU.

## Architecture

```
[docker network: susutaku-e2e-backend]
  postgres (susutaku_e2e) ---- backend (:8991, sandboxed agent) ---- web (nginx, proxies /api)
                                     |                                    |
                                     v                                    v
                          mock-model (:8992, /api/inference)         playwright
                                     |
                        deterministic replies (no GPU, no network)
```

## Files

| Path | Purpose |
|---|---|
| `playwright/mock-model/server.js` | Mock of the local MLX model server: `/api/models`, `/api/models/select`, `/api/inference` |
| `docker/mock/Dockerfile.mock-model` | Mock model image (node:22-alpine, no deps) |
| `docker/compose/playwright-backend.yml` | postgres + mock-model + backend + web + playwright, zero published ports; backend/web healthchecks gate the runner |
| `playwright/tests/chat-mock.spec.ts` | Chat UI catalog, echo, toolcall round, summarize, `do task:` card flow, sandboxed agent run (gated on `E2E_MOCK_MODEL=1`) |
| `playwright/tests/chat-modal.spec.ts` | Chat as a global modal — same mock-model gating, but the compose `command` runs only `chat-mock` |

## Mock model reply rules (deterministic)

The mock inspects the prompt the backend's `ChatUseCase` assembles, in this
order:

| Prompt shape | Reply |
|---|---|
| Offers tools (`You HAVE web tools`) and contains `do task:` | `TOOL: BOARD_LIST` — first step of the scripted card flow below |
| Offers tools, no `Tool results:` yet | `TOOL: SHELL echo toolcall-ok` — one tool line, so the backend runs it in its sandbox and re-prompts |
| Carries `Tool results:` and contains `do task:` | Next step of the scripted flow (below), keyed on the number of `Tool results:` headers |
| Carries `Tool results:` | `TOOLCALL-OK: shell tool ran and returned toolcall-ok.` — final answer, proves the round-trip |
| Contains `summarize` | `MOCK-SUMMARY: e2e context compacted by the mock model.` — the compact/summary case |
| Contains a `[context: …]` line (tools off) | `echo: [context: …]` — the context line is echoed back, wherever it sits in the prompt |
| Anything else | `echo: <last prompt line>` — plain chat fallback |

**Scripted `do task:` flow** (card+agent path; pipelines are gone — cards now
run their assigned agent): round 0 `TOOL: BOARD_LIST`, round 1
`TOOL: CARD_FIND <title>`, then find-or-create — `TOOL: CARD_CREATE <project>
<title> | plan: <topic>` if no card matched, else round 2
`TOOL: CARD_AGENT <card> default` (always assigns an agent), finally
`TASK-OK: task stored on card <id>: <title>`. Saying the same task again
must reuse the card, not duplicate it — the spec asserts exactly one card
per topic.

## The loop

```sh
# 1. Build + run the whole stack (exits with the test status)
docker compose -f docker/compose/playwright-backend.yml up --build \
  --exit-code-from playwright

# 2. Reset (the postgres service owns susutaku_e2e; no drop/create from the suite)
docker compose -f docker/compose/playwright-backend.yml down -v
```

Only the `chat-mock` spec runs here (`command: ["chat-mock"]`); the full
UX/snapshot suite stays on the host-backend stack from
`playwright-workflow.md`.

## What the suite covers

1. **Catalog** — `/api/models` lists the mock model (`e2e-mock`), selected.
2. **Chat UI echo** (search off) — message in, `echo: [context: …]` reply
   bubble out (the context toggle is switched on so the reply is deterministic).
3. **Toolcall round** (search auto) — mock answers `TOOL: SHELL …`, the
   backend executes it inside its sandbox, re-prompts, and the final
   `TOOLCALL-OK` answer lands in the UI.
4. **Summary (compact)** — `POST /api/chat` with a `summarize …` message
   returns the `MOCK-SUMMARY:` reply.
5. **`do task:` card flow** — the mock walks `BOARD_LIST → CARD_FIND →
   CARD_CREATE → CARD_AGENT` through the tool loop; the spec then asserts the
   card exists exactly once, with the assigned agent, and that repeating the
   task reuses the card (card cron scheduling was removed — routines are a
   separate owner-handled entity).
6. **Agent in that backend** — `POST /api/manager/agents` spawns a sandboxed
   agent, `/run echo agent-run-ok` executes and its output is asserted.
   Skipped on Docker VMs without `/dev/fuse`/overlay mounts
   (`E2E_SKIP_SANDBOX=1`), same for the toolcall round.

## Environment (all inside compose)

| Var | Value | Meaning |
|---|---|---|
| `SUSUTAKU_LOCAL_MODEL_URL` | `http://mock-model:8992` | Backend → mock model |
| `DATABASE_URL` | `postgres://…@postgres:5432/susutaku_e2e` | Backend → compose postgres (DB created by `POSTGRES_DB` on first boot) |
| `E2E_SKIP_DB_LIFECYCLE` | `1` | The suite does not drop/create the DB; `down -v` resets |
| `E2E_MOCK_MODEL` | `1` | Gates `chat-mock.spec.ts` (other stacks skip it) |
| `E2E_SKIP_SANDBOX` | `1` | Skips the sandbox-dependent tests (toolcall round, agent spawn/run) on Docker VMs that can't run nested podman |
| `E2E_ADMIN_USER` / `E2E_ADMIN_PASSWORD` | `owner` / `owner` | The backend seeds the default owner on connect; `helpers.ts` rotates the forced password (`E2E_ADMIN_NEW_PASSWORD=owner-owner-1`) and creates `e2e-tester` |

## Notes

- The backend still needs `security_opt: [seccomp=unconfined,
  apparmor=unconfined]` — the agent sandbox refuses to run unsandboxed (by
  design, no fallback).
- **Building** `Dockerfile.backend` compiles sqlx macros against the *host*
  postgres from `docker/compose/base.yml` (port 5434) — keep it up while
  building. Runtime uses the compose postgres only. The compose `build:` must
  set `target: backend-runtime` (the Dockerfile's last stage is a hub stub).
- There is no `/compact` command in the backend yet; the summary case rides
  `/api/chat` with the mock replying `MOCK-SUMMARY:`. A real endpoint would
  fold conversation context into one summary inference call.
