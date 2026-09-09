# Playwright E2E + UX Workflow

Dockerized Playwright suite for `web_ui`. Tests run inside Docker, publish **no
ports** (all traffic stays on the internal compose network), and are configured
purely through compose environment variables. Screenshots flow back to the host
so UX/UI can be reviewed and improved after every run.

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
| `docker/Dockerfile.playwright` | Playwright runner image (browsers + deps baked in) |
| `docker/docker-compose.playwright.yml` | postgres + web + playwright, zero published ports |
| `playwright/playwright.config.ts` | baseURL from `PLAYWRIGHT_BASE_URL` env |
| `playwright/global-setup.ts` | Fresh `susutaku_e2e` DB lifecycle on the internal compose postgres |
| `playwright/setup.ts` / `playwright/teardown.ts` | globalSetup / globalTeardown hooks |
| `playwright/tests/helpers.ts` | Seeds the e2e user (bootstrap or admin-created) |
| `playwright/tests/fixtures.ts` | `login` fixture: fresh logged-in page per test |
| `crates/kanban-rs/examples/hash_password.rs` | Prints an argon2 hash to seed the e2e admin (one-time) |
| `playwright/tests/auth.spec.ts` | Login redirect / bad-credential flows |
| `playwright/tests/kanban.spec.ts` | Board loads; workspace+project API round-trip |
| `playwright/tests/card-detail.spec.ts` | Card detail: two-pane layout, priority/deadline, tabs, agent mention chat, run history |
| `playwright/tests/ux-snapshots.spec.ts` | Full-page screenshots of every route |
| `playwright/screenshots/` | UX review images (bind-mounted from the container) |

## Environment (all inside compose — nothing hardcoded in tests)

| Var | Default | Meaning |
|---|---|---|
| `PLAYWRIGHT_BASE_URL` | `http://web` | Web from inside the network |
| `BACKEND_PORT` | `8991` | Host backend port the web proxy targets |
| `E2E_ADMIN_USER` / `E2E_ADMIN_PASSWORD` | `admin` / `adminadmin` | Admin used to create the e2e user when bootstrap is unavailable |
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

## One-time setup: seed the e2e admin (host backend only)

The e2e stack proxies `/api` to the host backend (Metal requirement), so the
host dev DB needs an admin whose credentials the suite knows. Default env:
`e2e-admin` / `e2e-admin-e2e-admin` (role `admin`, no forced password change).

```sh
HASH=$(cargo run -q -p kanban-rs --example hash_password -- 'e2e-admin-e2e-admin')
docker exec susutaku-postgres-1 psql -U susutaku -d susutaku -c \
  "INSERT INTO users (username, password_hash, role, must_change_password) \
   VALUES ('e2e-admin', '$HASH', 'admin', FALSE) \
   ON CONFLICT (username) DO UPDATE SET password_hash = EXCLUDED.password_hash;"
```

Override via compose env `E2E_ADMIN_USER` / `E2E_ADMIN_PASSWORD` for any other
admin account. The suite then creates its own `e2e-tester` user through the
public `POST /api/auth/users` endpoint.

## The loop

```sh
# 1. Start the host backend (MLX/Metal requirement)
cargo run -p backend

# 2. Run the whole suite in Docker (builds, runs, exits with test status)
docker compose -f docker/docker-compose.playwright.yml up --build \
  --exit-code-from playwright

# 3. Review UX snapshots
open playwright/screenshots/pages/

# 4. Improve UX/UI in web_ui/, then re-run step 2.
#    Intentional visual change? Refresh baselines:
docker compose -f docker/docker-compose.playwright.yml run --rm \
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
- **Seeding via public API only.** The e2e user is created through the real
  bootstrap/admin endpoints — no DB fixtures to keep in sync. Only the admin
  account itself is seeded once (argon2 hash, same crate the backend uses).
- **Exit code propagation.** `--exit-code-from playwright` makes the compose run
  CI-usable directly.

## Known findings (first UX review, 2026-09)

- `settings` page shows a raw `404 not found` from `/api/settings/client-env`
  on load. Verified the route exists in `backend/src/api.rs` but the *running*
  host backend binary is stale (its OpenAPI lacks the path) — rebuild/restart
  `cargo run -p backend` to fix; consider a friendlier inline error state too.
- `login` page renders the form high on the page; could be vertically centered.
- `kanban` empty columns (e.g. `Done 0`) have no drop affordance/hint.
