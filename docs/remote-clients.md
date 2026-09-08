# Remote server / client machines — usage & flow

Architecture: one **server** hosts the models (`model-server`: inference HTTP
+ TCP command hub) and one or more **clients** register with it
(`backend` client node). The server dispatches process commands to clients;
each client runs them inside its platform sandbox and returns the output.
The web **backend** on any machine never runs model code inline — it is a
pure provider client of `model-server`.

## Machine roles

| Role | Binary | Runs | Register as |
|---|---|---|---|
| server (model host) | `model-server` | inference engine + command hub (HTTP :8992, TCP :8993) | — |
| worker client | `backend` | client node + sandbox (seatbelt / WSL / namespaces) | yes |
| model client | `model-server` | its own models + hub | n/a (it *is* a server) |
| web/app host | `backend` | HTTP API :8991 + client node, remote inference | yes |

## 1. Install flow (client machine)

```mermaid
flowchart TD
    A[client machine] --> B[curl install.sh?role=auto from server :8991]
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
    I --> J[cargo build --release<br/>model-server + backend]
    J --> K{role}
    K -->|model| L[start model-server<br/>logs ~/.susutaku/model-server.log]
    K -->|worker| M[start backend with<br/>SUSUTAKU_MODEL_SERVER_URL + SUSUTAKU_HUB_ADDR<br/>logs ~/.susutaku/backend.log]
```

## 2. Registration & command dispatch (TCP, proto-rs)

```mermaid
sequenceDiagram
    participant C as client (backend)
    participant H as hub (model-server :8993)
    participant S as server operator / API
    C->>H: TCP connect + Register { hostname, os }
    H->>H: registry[client_id] = connection
    H-->>C: ok
    S->>H: POST /api/clients/{id}/command { cmd }
    H->>C: Envelope { id, Command { cmd } }
    C->>C: run cmd in sandbox<br/>(macOS: seatbelt · Windows: WSL/restricted token · Linux: namespaces)
    C->>H: Envelope { id, Result { output } }
    H-->>S: 200 { output }
    Note over C,H: heartbeat ping/pong keeps the link alive;<br/>client reconnects every 3 s if the hub drops
```

## 3. Inference flow (no inline model code anywhere else)

```mermaid
sequenceDiagram
    participant U as user / web UI
    participant B as backend :8991
    participant M as model-server :8992
    U->>B: chat / generate request
    B->>M: POST /api/inference { prompt, max_tokens, tok, think }
    M->>M: engine thread (MLX q4 / GGUF) generates
    M-->>B: { model, text, stats }
    B-->>U: reply
    U->>B: POST /api/models/select { name }
    B->>M: POST /api/models/select { name }
    M-->>B: ok
    B-->>U: ok
```

## 4. Commands

Server machine (with a downloaded model):

```sh
make download-model                    # ≥30 GiB 4-bit MLX checkpoint into models/
./target/release/model-server          # HTTP :8992 + hub :8993
```

Client machine (one line):

```sh
curl -fsSL "http://<server>:8991/install.sh?role=auto&server=http://<server>:8992" | sh
```

Verify + drive a client from the server:

```sh
curl http://localhost:8992/api/clients
curl -X POST http://localhost:8992/api/clients/<id>/command \
     -H 'content-type: application/json' -d '{"cmd":"echo hi"}'
```

Override points (env, no .env files): `MODEL_SERVER_HTTP_PORT`,
`MODEL_SERVER_TCP_PORT` (server) · `SUSUTAKU_MODEL_SERVER_URL`,
`SUSUTAKU_HUB_ADDR` (client).
