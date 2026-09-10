# Playwright E2E Workflow: Backend in Docker + Mock Model

Companion to `playwright-workflow.md` (which runs the backend on the host
because real MLX inference needs macOS Metal). This stack has **no host
dependency at all**: postgres, the backend, the model server and the
playwright runner all live on the internal compose network, and the model is a
deterministic mock — so chat, toolcalling, summary and the agent path can be
tested end-to-end in CI without a GPU.

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
| `docker/Dockerfile.mock-model` | Mock model image (node:22-alpine, no deps) |
| `docker/docker-compose.playwright-backend.yml` | postgres + mock-model + backend + web + playwright, zero published ports |
| `playwright/tests/chat-mock.spec.ts` | Chat UI echo, toolcall round, summary, agent run (gated on `E2E_MOCK_MODEL=1`) |

## Mock model reply rules (deterministic)

The mock inspects the prompt the backend's `ChatUseCase` assembles:

| Prompt shape | Reply |
|---|---|
| Offers tools (`TOOL_INSTRUCTION`), no `Tool results:` yet | `TOOL: SHELL echo toolcall-ok` — one tool line, so the backend runs it in its sandbox and re-prompts |
| Carries `Tool results:` | `TOOLCALL-OK: shell tool ran and returned toolcall-ok.` — final answer, proves the round-trip |
| Contains `summarize` | `MOCK-SUMMARY: …` — the compact/summary case |
| Anything else (search off) | `echo: <message>` — plain chat, the message is the last prompt line |

Reply order matters: tool round 1 → tool round 2 → summary → echo, so
behaviour is stable regardless of the user's message text.

## The loop

```sh
# 1. Build + run the whole stack (exits with the test status)
docker compose -f docker/docker-compose.playwright-backend.yml up --build \
  --exit-code-from playwright

# 2. Reset (the postgres service owns susutaku_e2e; no drop/create from the suite)
docker compose -f docker/docker-compose.playwright-backend.yml down -v
```

Only the `chat-mock` spec runs here (`command: ["chat-mock"]`); the full
UX/snapshot suite stays on the host-backend stack from
`playwright-workflow.md`.

## What the suite covers

1. **Catalog** — `/api/models` lists the mock model, selected.
2. **Chat UI echo** (search off) — message in, `echo:` reply bubble out.
3. **Toolcall round** (search auto) — mock answers `TOOL: SHELL …`, the
   backend executes it inside its Linux rootless sandbox, re-prompts, and the
   final `TOOLCALL-OK` answer lands in the UI.
4. **Summary (compact)** — `POST /api/chat` with a `summarize …` message
   returns the `MOCK-SUMMARY:` reply.
5. **Agent in that backend** — `POST /api/manager/agents` spawns a sandboxed
   agent, `/run echo agent-run-ok` executes and its output is asserted.

## Environment (all inside compose)

| Var | Value | Meaning |
|---|---|---|
| `SUSUTAKU_LOCAL_MODEL_URL` | `http://mock-model:8992` | Backend → mock model |
| `DATABASE_URL` | `postgres://…@postgres:5432/susutaku_e2e` | Backend → compose postgres (DB created by `POSTGRES_DB` on first boot) |
| `E2E_SKIP_DB_LIFECYCLE` | `1` | The suite does not drop/create the DB; `down -v` resets |
| `E2E_MOCK_MODEL` | `1` | Gates `chat-mock.spec.ts` (other stacks skip it) |
| `E2E_ADMIN_USER` / `E2E_ADMIN_PASSWORD` | `owner` / `owner` | The backend seeds the default owner on connect; `helpers.ts` rotates the forced password (`E2E_ADMIN_NEW_PASSWORD=owner-owner-1`) and creates `e2e-tester` |

## Notes

- The backend still needs `security_opt: [seccomp=unconfined,
  apparmor=unconfined]` — the agent sandbox refuses to run unsandboxed (by
  design, no fallback).
- **Building** `Dockerfile.backend` compiles sqlx macros against the *host*
  postgres from `docker/docker-compose.yml` (port 5434) — keep it up while
  building. Runtime uses the compose postgres only.
- There is no `/compact` command in the backend yet; the summary case rides
  `/api/chat` with the mock replying `MOCK-SUMMARY:`. A real endpoint would
  fold conversation context into one summary inference call.
