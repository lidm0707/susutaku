# Playwright E2E + UX Workflow

Dockerized Playwright suite for `web_ui`. Tests run inside Docker, publish **no
ports** (all traffic stays on the internal compose network), and are configured
purely through compose environment variables. Screenshots flow back to the host
so UX/UI can be reviewed and improved after every run.

For the fully-containerized variant (backend in Docker + mock model, no host
dependency), see `playwright-backend-workflow.md`.

## Architecture

```
host: cargo run -p backend        # MLX needs macOS Metal, stays on host :8991
      |
      | BACKEND_URL (env in compose)
      v
[docker network: susutaku-e2e]
  postgres ---- web (nginx, proxies /api -> host backend) ---- playwright
                                                  screenshots via bind mounts
                                                  v
                              playwright/screenshots/ + test-results/ (host)
```

## Files

| Path | Purpose |
|---|---|
| `docker/test/Dockerfile.playwright` | Playwright runner image (browsers + deps baked in) |
| `docker/compose/playwright.yml` | postgres + web + playwright, zero published ports |
| `playwright/playwright.config.ts` | baseURL from `PLAYWRIGHT_BASE_URL` env (default `http://localhost:3334`) |
| `playwright/global-setup.ts` | Fresh `susutaku_e2e` DB lifecycle on the internal compose postgres |
| `playwright/setup.ts` / `playwright/teardown.ts` | globalSetup / globalTeardown hooks |
| `playwright/tests/helpers.ts` | Seeds the e2e user through the auth API (direct login → bootstrap → admin login + forced password rotation) |
| `playwright/tests/fixtures.ts` | `login` fixture: fresh logged-in page per test |
| `playwright/tests/auth.spec.ts` | Login redirect / bad-credential flows |
| `playwright/tests/task.spec.ts` | Board loads; workspace+project API round-trip |
| `playwright/tests/card-detail.spec.ts` | Card detail: two-pane layout, priority/deadline, tabs, comments, agent mention chat (route-intercepted), inline run + history timeline |
| `playwright/tests/card-run.spec.ts` | API-only: card runs its assigned agent via `POST /api/task/cards/{id}/run`; no agent → failed run lands in history; agent assign/read round-trip |
| `playwright/tests/chat-mock.spec.ts` | Mock-model stack: catalog, echo, toolcall round, summarize, scripted `do task:` card flow, sandboxed agent spawn+run (gated on `E2E_MOCK_MODEL=1`) |
| `playwright/tests/chat-modal.spec.ts` | Chat as global modal (floating fab → dialog); same mock-model gating as `chat-mock` |
| `playwright/tests/chat-card-html.spec.ts` | Chat card chip (new card → chip → card page) + html fence preview/raw toggle; route-intercepted |
| `playwright/tests/chat-dock-resize.spec.ts` | Chat dock width resize persists across reloads |
| `playwright/tests/chat-real.spec.ts` | Real local model server, gated on `E2E_REAL_MODEL=1` (see `docker/compose/playwright-real.yml`) |
| `playwright/tests/graph.spec.ts` | ```plot fence renders a wgpu-wasm canvas (or 2D fallback) without wasm errors; chat intercepted |
| `playwright/tests/claude.spec.ts` | Claude provider: status endpoint, callback validation |
| `playwright/tests/settings.spec.ts` | Settings page client-env tab (server-detected host, workspace path) |
| `playwright/tests/dock.spec.ts` | Dock modals: runtime (MachinesModal component, renamed), workspace creation (ProfileModal → PromptModal) |
| `playwright/tests/worker.spec.ts` | Auth guards for cronjobs; card cron scheduling is gone — the scheduler now picks up **routines** (`/api/routines`, owner-handled) |
| `playwright/tests/ux-snapshots.spec.ts` | Full-page screenshots of every route (`/`, `/task`, `/prompts`, `/settings`) |
| `playwright/tests/walkthrough.spec.ts` | Step-by-step walkthrough capture into `screenshots/walkthrough/` |
| `playwright/tests/deploy-check.spec.ts` | Ad-hoc live-deploy check, gated on `E2E_DEPLOY_CHECK=1` |
| `playwright/screenshots/` | UX review images (bind-mounted from the container) |

## Environment (all inside compose — nothing hardcoded in tests)

| Var | Default | Meaning |
|---|---|---|
| `PLAYWRIGHT_BASE_URL` | `http://web` in compose / `http://localhost:3334` bare | Web from inside the network |
| `BACKEND_PORT` | `8991` | Host backend port the web proxy targets |
| `E2E_ADMIN_USER` / `E2E_ADMIN_PASSWORD` | `owner` / `owner` (helpers); compose passes `e2e-admin` / `e2e-admin-e2e-admin` | Admin used to create the e2e user when bootstrap is unavailable |
| `E2E_ADMIN_NEW_PASSWORD` | `owner-owner-1` | Rotation target for the auto-seeded owner's forced password change |
| `E2E_USER` / `E2E_PASSWORD` | `e2e-tester` / `e2e-e2e-e2e` | Credentials tests log in with |
| `E2E_SKIP_DB_LIFECYCLE` | unset | Set to `1` to skip the fresh-DB create/drop |
| `E2E_PG_HOST` | `postgres` | Internal postgres service for the e2e DB |
| `E2E_PG_PORT` | `5432` | Internal postgres port |
| `E2E_DB_NAME` | `susutaku_e2e` | Fresh database created then dropped per run |

## Fresh database per run (internal compose network)

`globalSetup` drops + creates a dedicated `susutaku_e2e` database on the
compose `postgres` service, reachable only over the internal docker network
(`E2E_PG_HOST=postgres:5432`, no published ports). `globalTeardown` terminates
connections and drops it. The playwright container talks to postgres directly
via the `pg` client — no `docker exec`, no host access. Migrations run
automatically on backend connect, so the DB needs no seeding beyond the
suite's own API bootstrap.

## Seeding: no one-time setup

The backend auto-seeds a default `owner` / `owner` account (role `owner`,
`must_change_password = TRUE`) on every DB connect
(`Store::ensure_default_admin` in `backend/src/infra/postgres/user.rs`). There is no
manual psql/hash step anymore.

`tests/helpers.ts` seeds the `e2e-tester` user through the real API, in order:

1. Try logging in as `e2e-tester` directly (already seeded by a past run) — done.
2. Zero users → unauthenticated bootstrap `POST /api/auth/users` — done.
3. Otherwise log in as the admin (`E2E_ADMIN_USER`/`E2E_ADMIN_PASSWORD`,
   retrying with `E2E_ADMIN_NEW_PASSWORD` if the password was already
   rotated), complete the forced password change via
   `POST /api/auth/change-password`, and create `e2e-tester`
   (409 / "already taken" counts as success).

## The loop

```sh
# 1. Start the host backend (MLX/Metal requirement)
cargo run -p backend

# 2. Run the whole suite in Docker (builds, runs, exits with test status)
docker compose -f docker/compose/playwright.yml up --build \
  --exit-code-from playwright

# 3. Review UX snapshots
open playwright/screenshots/pages/

# 4. Improve UX/UI in web_ui/, then re-run step 2.
#    Intentional visual change? Refresh baselines:
docker compose -f docker/compose/playwright.yml run --rm \
  -e PLAYWRIGHT_BASE_URL=http://web playwright npx playwright test --update-snapshots
```

## Why this shape (the "GOAT" checklist)

- **Tests run in Docker, always.** Same browsers, fonts, and locale on every
  machine and in CI — snapshot diffs stay meaningful.
- **Zero published ports.** The test stack can't collide with the dev stack
  (5434/3334) or leak during CI; access is only via the compose network.
- **Env-only configuration.** Everything differs-by-environment is a compose
  variable; the image and tests contain no secrets or URLs.
- **Host backend by design.** MLX cannot run under Linux containers, so the
  proxy contract (`/api -> BACKEND_URL`) is tested exactly as it ships, while
  inference stays on Metal.
- **Feature tests + UX snapshots in one run.** Functional failures and visual
  regressions are caught by the same `docker compose up`, no separate tooling.
- **Screenshots as artifacts, baselines in git.** `screenshots/pages/*.png` are
  for human UX review; `tests/__screenshots__/` baselines gate regressions.
- **Seeding via public API only.** The admin account is auto-seeded by the
  backend itself (owner/owner + forced change, rotated by `helpers.ts`);
  the e2e user is created through the real auth endpoints — no DB fixtures,
  no argon2 hash to paste by hand.
- **Exit code propagation.** `--exit-code-from playwright` makes the compose run
  CI-usable directly.

## Known findings (first UX review, 2026-09)

- `settings` page shows a raw `404 not found` from `/api/settings/client-env`
  on load. Verified the route exists in `backend/src/api.rs` but the *running*
  host backend binary is stale (its OpenAPI lacks the path) — rebuild/restart
  `cargo run -p backend` to fix; consider a friendlier inline error state too.
- `login` page renders the form high on the page; could be vertically centered.
- `task` empty columns (e.g. `Done 0`) have no drop affordance/hint.
