# Backend, client machines & web UI — usage & flow

Three kinds of machines:

- **backend** — the brain host: serves the web UI/API on :8991, runs the
  manager process (one sandbox per agent), and connects to `local-model`
  for inference. Never runs model code inline.
- **client machine** — runs the installed `backend` binary in *client mode*:
  it registers with the local-model hub over TCP and executes dispatched
  commands in the rootless podman sandbox (same as the backend — see
  `docs/podman-sandbox.md`). It runs no web UI and no models.
- **web UI** — any browser pointed at `http://<backend>:8991`; talks only to
  the backend, never to local-model or clients directly.

`local-model` stays a standalone box (inference + TCP hub) that both the
backend and client machines talk to.

## Machine overview

```mermaid
flowchart TD
    W[web UI<br/>any browser] -->|HTTP :8991| B[backend<br/>API + manager process]
    B -->|HTTP :8992 inference| M[local-model<br/>models + TCP hub :8993]
    B -->|TCP Register| M
    M -->|Command| C[client machine<br/>sandbox executor]
    C -->|Result| M
    C -.->|install.sh probe :8991| B
```

| Machine | Binary | Ports | Job |
|---|---|---|---|
| backend | `backend` | :8991 | web API, manager process (agent sandboxes), remote inference client |
| client machine | `backend` (client mode) | — | registers with hub, runs commands in sandbox |
| local-model | `local-model` | :8992 + :8993 | inference engine + client hub |
| web UI | browser | — | operator console |

## 1. Install flow (client machine)

```mermaid
flowchart TD
    A[client machine] --> B[curl install.sh?role=auto from backend :8991]
    B --> C{probe machine}
    C --> C1[OS / arch check<br/>macos linux windows]
    C --> C2[sandbox shell deps check<br/>zsh / bash / powershell]
    C --> C3[RAM check via sysctl / meminfo]
    C1 & C2 & C3 --> D{role decision}
    D -->|auto + Apple Silicon ≥32GiB| E[model]
    D -->|auto otherwise| F[worker]
    D -->|model forced but incapable| X[abort install]
    E & F --> G{cargo installed?}
    G -->|no| H[rustup install]
    G -->|yes| I
    H --> I[shallow clone repo to ~/.susutaku/src]
    I --> J[cargo build --release<br/>local-model + backend]
    J --> K{role}
    K -->|model| L[start local-model<br/>logs ~/.susutaku/local-model.log]
    K -->|worker| M[start backend client mode<br/>SUSUTAKU_LOCAL_MODEL_URL + SUSUTAKU_HUB_ADDR<br/>logs ~/.susutaku/backend.log]
```

## 2. Client machine: register & run dispatched commands (TCP, proto-rs)

Registration requires full machine metadata (`proto_rs::ClientMeta`:
hostname, os, arch, role, ram_gib). The installer probe decides `role`
(model|worker) and measures `ram_gib`, then exports them as
`SUSUTAKU_CLIENT_ROLE` / `SUSUTAKU_CLIENT_RAM_GIB` before launching the
client node; the client node refuses to run without them, and the hub
closes the connection of any Register that carries missing/invalid
metadata.

```mermaid
sequenceDiagram
    participant C as client machine
    participant H as hub (local-model :8993)
    participant B as backend
    C->>H: TCP connect + Register { hostname, os }
    H->>H: registry[client_id] = connection
    H-->>C: ok
    B->>H: POST /api/clients/{id}/command { cmd }
    H->>C: Envelope { id, Command { cmd } }
    C->>C: run cmd in sandbox<br/>(rootless podman, per-agent work tree if addressed)
    C->>H: Envelope { id, Result { output } }
    H-->>B: 200 { output }
    Note over C,H: heartbeat keeps the link alive,<br/>client reconnects every 3 s if the hub drops
```

## 3. Web UI → backend → local-model (inference)

```mermaid
sequenceDiagram
    participant U as web UI
    participant B as backend :8991
    participant M as local-model :8992
    U->>B: chat / generate request
    B->>M: POST /api/inference { prompt, max_tokens, tok, think }
    M->>M: engine thread (MLX q4 / GGUF) generates
    M-->>B: { model, text, stats }
    B-->>U: reply
```

## 4. Manager process on the backend — one sandbox per agent

```mermaid
sequenceDiagram
    participant U as web UI / operator
    participant B as backend :8991
    participant S as sandbox per agent
    U->>B: POST /api/manager/agents { agent }
    B->>S: Sandbox::new_in(work/agents/<agent>)
    B-->>U: { agent, work_tree }
    U->>B: POST /api/manager/agents/{agent}/run { cmd }
    B->>S: sandbox.run(cmd) + transcript
    B-->>U: { output } (kept as pending result)
    U->>B: POST /api/manager/agents/{agent}/finish
    B-->>U: TaskOutcome { result, state, work_tree }
    B->>S: purge sandbox + work tree
```

- `spawn` is idempotent — an already-running agent keeps its work tree.
- Stale non-empty work trees (crashed run, backend restart) are reclaimed on
  the next spawn of the same agent name.
- `state` returned on finish carries the cwd + full transcript, ready to be
  persisted (e.g. into the kanban card's `agent_state`).

## 5. Commands

Backend machine (with a local model server available):

```sh
./target/release/backend               # API :8991 + manager process + client node
```

Client machine (one line) — `server` is the backend URL; the installer expects
the local-model server on that same host at :8992:

```sh
curl -fsSL "http://<backend-host>:8991/install.sh?role=auto&server=http%3A%2F%2F<backend-host>%3A8991" | sh
```

Web UI: open `http://<backend-host>:8991` in a browser (vite dev: `make web`
on :5173, proxies to :8991).

Verify + drive a client machine:

```sh
curl http://<local-model>:8992/api/clients
curl -X POST http://<local-model>:8992/api/clients/<id>/command \
     -H 'content-type: application/json' -d '{"cmd":"echo hi"}'
```

Drive an agent sandbox on the backend:

```sh
curl http://localhost:8991/api/agents/whereis/fix-login   # which machine holds the agent
curl http://localhost:8991/api/manager/agents
curl -X POST http://localhost:8991/api/manager/agents -H 'content-type: application/json' -d '{"agent":"fix-login"}'
curl -X POST http://localhost:8991/api/manager/agents/fix-login/run -H 'content-type: application/json' -d '{"cmd":"ls"}'
curl -X POST http://localhost:8991/api/manager/agents/fix-login/finish
```

Override points (env, no .env files): `LOCAL_MODEL_HTTP_PORT`,
`LOCAL_MODEL_TCP_PORT` (local-model) · `SUSUTAKU_LOCAL_MODEL_URL`,
`SUSUTAKU_HUB_ADDR` (backend / client machine).

## 6. Known gaps & planned design

### 6.1 Resource check before running an agent (queue)

Today `ManagerProcess::spawn` checks only the in-memory agent map — no RAM
check, no cap, no queue. Planned:

- `MAX_AGENTS` cap (env-tunable) on concurrently running agent sandboxes.
- Free-RAM floor checked at spawn (same probe as the installer, in Rust:
  `sysctl hw.memsize` on macOS, `/proc/meminfo` on Linux).
- Spawns that arrive while at capacity go into a bounded wait queue
  (`queue-rs::BoundedQueue`) and start FIFO as running agents finish.

```mermaid
flowchart TD
    R[spawn agent request] --> F{agents at MAX_AGENTS?}
    F -->|yes| Q[wait queue FIFO<br/>queue-rs BoundedQueue]
    Q -->|slot frees on finish| G
    F -->|no| G{free RAM >= floor?}
    G -->|no| Rj[reject: not enough resources]
    G -->|yes| S[new_in work/agents/<agent>]
```

### 6.2 Agent-aware commands for client machines — IMPLEMENTED

Implemented as of the podman sandbox work:

- `proto-rs` `Kind::Command` carries `agent: String` (empty = the
  process-global sandbox).
- The client node keeps a `ManagerProcess`: a command addressed to agent X
  spawns `work/agents/X` on the client and runs there (own work tree,
  transcript, podman sandbox).
- `POST /api/clients/{id}/command` accepts `{ cmd, agent }`.
- `GET /api/clients/{id}/agents` returns the agent names the client's manager
  holds (proto `Kind::AgentNames` → `AgentNamesResult` round trip).
- `POST /api/clients/{id}/kick` disconnects the client from the hub.
- New backend endpoints drive shared machines by hostname:
  `POST /api/machines/{hostname}/agents/{agent}/run { cmd }`.
- Web UI machines modal (agent sandboxes tab) lists every machine with the
  agents it runs (`agents: …`) and its sandbox count; remote machines can be
  kicked (disconnected) from there.
  machines; picking one opens an agent console (agent name + command →
  output) that spawns/runs agents on that machine.
- Verified end-to-end by the client-test stack:
  `hub_stub` dispatches a global probe **and** an agent probe
  (`test-agent`) to every registering client;
  `docker compose -f docker/compose/client-test.yml logs hub | grep RESULT`
  must show `STUB-HUB-ROUNDTRIP` and `AGENT-ROUNDTRIP` from each client,
  and `work/agents/test-agent` must exist inside the client container.

```mermaid
sequenceDiagram
    participant B as backend
    participant H as hub
    participant C as client machine
    participant A as ManagerProcess on client
    B->>H: Command { agent, cmd }
    H->>C: Envelope { id, Command { agent, cmd } }
    C->>A: run(agent, cmd)
    A->>A: sandbox work/agents/<agent> (podman)
    C->>H: Result { output }
```

```mermaid
sequenceDiagram
    participant B as backend
    participant H as hub
    participant C as client machine
    participant A as ManagerProcess on client
    B->>H: Command { agent, cmd }
    H->>C: Envelope { id, Command { agent, cmd } }
    C->>A: run(agent, cmd)
    A->>A: sandbox work/agents/<agent>
    C->>H: Result { output }
```

### 6.3 Failure path for a wrong agent

Today a failing `run` returns `Err` but nothing records it, and only Windows
has a command timeout — a hung command blocks the agent's slot forever.
Recovery is passive (stale tree reclaimed on next spawn). Planned:

- Run-with-timeout on all platforms (macOS/Linux get the same
  `SandboxLimits.timeout_secs` idea Windows already has); on timeout the
  process tree is killed and the command fails.
- `fail(agent, reason)` path: marks the agent `Failed`, records the error
  into its transcript and pending result, then `finish` returns that state
  so callers can audit what went wrong.
- Startup sweep of orphaned `work/agents/*` dirs (agents not in the map),
  mirroring `AgentSandbox::sweep()`.

```mermaid
flowchart TD
    R[run agent cmd] --> T{timeout?}
    T -->|yes| K[kill process tree]
    T -->|no, ok| Ok[output kept as result]
    T -->|no, error| K2[error returned]
    K --> F[fail agent: mark Failed + transcript entry]
    K2 --> F
    F --> Fi[finish returns state incl. failure]
    Fi --> P[purge sandbox + work tree]
```
