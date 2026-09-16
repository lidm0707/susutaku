# How the Agent Works from the Web UI

How a chat message typed into `web_ui` becomes an agentic, tool-using answer.

## The big picture

```mermaid
graph TD
    A[web_ui chat page] -->|POST /api/chat/zai/stream or /api/chat/zai| B[axum api.rs chat handler]
    B --> C[ChatUseCase - app/chat.rs]
    C -->|Prompt.build| D[Inference engine - RemoteModel]
    D -->|reply text| C
    C -->|TOOL: line or coding XML| E{Tool?}
    E -->|SEARCH| F[DuckDuckGo Searcher]
    E -->|FETCH| G[PageFetcher]
    E -->|SHELL / CODING| H[AgentSandbox runner - core-agent]
    E -->|MATH / LSP / GIT| I[host + work-tree services]
    E -->|CARD_* / BOARD_LIST / CARD_FIND| J[BoardService - task-rs store]
    E -->|AGENT_RUN| K[ManagerRun - manager slot]
    F --> C
    G --> C
    H --> C
    I --> C
    J --> C
    K --> C
    C -->|ToolEvent stream + final answer| A
```

## Step by step

1. **User sends a message** — the web UI calls `POST /api/chat/zai` (JSON
   reply), `POST /api/chat/zai/stream` (SSE), or `POST /api/chat` (local MLX
   model). The request carries `{ message, agent?, thread_id?, card_id?,
   run_id?, max_tokens?, search?, tokenizer?, think? }`.
   `search` controls tooling: `"off"` = plain chat, `"on"` (force) = always
   web-search first, `"auto"` (default) = the model decides via tools.
   `run_id` enables cancellation via `POST /api/chat/zai/cancel`.

2. **Prompt assembly** (`domain/service/prompt.rs`) — `Prompt::build` layers
   one instruction block per enabled tool family (`Section` enum + match):
   `TOOL_WEB_INSTRUCTION`, `TOOL_SHELL_INSTRUCTION`, `TOOL_CODING_INSTRUCTION`,
   `TOOL_MATH_INSTRUCTION`, `TOOL_GIT_INSTRUCTION`, `TOOL_LSP_INSTRUCTION`,
   `TOOL_AGENT_RUN_INSTRUCTION`, `BOARD_TOOL_INSTRUCTION`, plus `TOOL_RULES`.
   All tools use plain `TOOL:` lines; the one exception is the coding tool,
   which replies with an `<invoke name="coding">` XML block (the only XML
   form allowed — the parser also accepts the `write_file` alias).

3. **Inference** — `ChatUseCase::infer` submits the prompt to the inference
   engine (`Inference` port; `RemoteModel` wired in `main.rs`).

4. **Agentic tool loop** (Zed-style, `app/chat.rs::execute`):
   - While the reply parses as a `ToolCall` (`ToolCall::parse` handles both
     `TOOL:` lines and XML invokes) and the round budget
     (`TOOL_ROUNDS_MAX = 8`) is not exhausted, the tool runs:
     - `TOOL: SEARCH <query>` → `DuckDuckGo` (blocking task), top
       `CONTEXT_RESULTS_MAX = 5` results formatted into context,
     - `TOOL: FETCH <url>` → `PageFetcher` downloads the page into context,
     - `TOOL: SHELL <cmd>` → executed inside the **agent sandbox**; output is
       scanned for produced files (max `ARTIFACT_SCAN_MAX = 8` per run),
     - `TOOL: CODING` (XML block, alias `write_file`) → writes a file into the
       work tree (only offered when the work tree is a git repo),
     - `TOOL: MATH <subcommand> …` (alias `GEOMATH`) → exact geometry math,
     - `TOOL: GIT <OP>` → `CLONE`/`STATUS`/`DIFF` run on the host work tree;
       `BRANCH`/`COMMIT`/`PUSH`/`PR` run inside a named agent's container
       (selected by trailing `@<agent>` on the line),
     - `TOOL: LSP DEFINITION|REFERENCES|HOVER <path> <line> <col>` →
       rust-analyzer over the work tree,
     - `TOOL: CARD_FIND | BOARD_LIST | CARD_CREATE | CARD_AGENT | CARD_IMAGE |
       CARD_RUN` → task-board operations through `BoardService` (legacy
       `CARD_ROUTINE`/`CARD_ROUTINE_CLEAR` still write `card.cron`, but the
       scheduler no longer runs card cron — use `/api/routines`):
     - `TOOL: AGENT_RUN <agent> <cmd>` → runs one command as a named agent in
       its own sandbox (see `docs/agent-run-tool.md`).
   - Every finished call is emitted as a `ToolEvent` over a broadcast channel
     (`with_tool_events`); the SSE stream handler forwards them live to the UI.
   - Tool results join the context, the prompt is rebuilt, the model infers
     again. Disallowed tools yield `TOOL_DENIED`; a mid-loop inference error
     falls back to the last good reply.
   - When the budget is exhausted but the model still wants a tool, a final
     answer is forced without the tool offer.

5. **Response** — `ChatOutcome` is returned to the UI: model text, search
   flag, stats, tool trace, and any artifacts persisted as card resources
   (images ≤ `RESOURCE_IMAGE_MAX_BYTES` inlined as data URLs, text truncated
   to `RESOURCE_TEXT_MAX_CHARS`).

## The sandbox (shell tool)

`backend/src/infra/podman.rs` wraps `core_agent::sandbox::Sandbox` as
`AgentSandbox`:

- Created once at backend startup (`AgentSandbox::restore()`); stale sandbox
  dirs from dead processes are purged first.
- Every `TOOL: SHELL` command runs confined in that workspace (per-OS
  implementation under `crates/core-agent/src/sandbox/{macos,linux,windows}.rs`
  with limits: timeout, output size, process/memory caps, network policy).
- Coding threads get their own per-thread sandbox runner
  (`turn_runner` in `chat.rs`).
- The sandbox root doubles as the Codex/Claude CLI workspace
  (`codex_workspace` in `main.rs`).
- Ops endpoints: `GET /api/sandbox` (list dirs + liveness),
  `POST /api/sandbox/purge` (by pid), `POST /api/sandbox/sweep` (remove dead).

## Other agent paths from the web

- **Model selection** — `POST /api/models/select` switches the model the agent
  answers with; `GET /api/models` lists loadable ones (via `hf_loader`).
- **Manager agents** — named agents with their own work trees, spawned on
  demand; the chat reaches them with `TOOL: AGENT_RUN` (or
  `POST /api/manager/agents/{agent}/run` directly).
- **External CLI agents** — `/api/chat/codex` and `/api/chat/claude` route the
  same chat shape through Codex/Claude CLI after their respective auth flows.
- **Threads** — `GET/POST /api/chat/threads`, `GET/DELETE
  /api/chat/threads/{id}`; per-thread memory recall feeds past exchanges into
  the prompt.

## Task cards: agent runs

```mermaid
graph TD
    A[Run button] --> B[card_run::run_card]
    B --> C[Load card - NoSuchCard if missing]
    C --> D[Move card to in_progress status]
    D --> E{Agent assigned?}
    E -->|no| F[Failed run: no agent assigned to the card]
    E -->|yes| G{Inference engine configured?}
    G -->|no| H[Failed run: no inference engine configured]
    G -->|yes| I[Load AgentConfigRow by name]
    I -->|missing| F
    I -->|found| J[Build prompt: persona + instruction + output format + task title/description]
    J --> K[engine.submit - INFER_MAX_TOKENS]
    K -->|reply| L[RunRecord ok + output]
    K -->|error / dropped| M[RunRecord failed]
    F --> N[RunRecord]
    H --> N
    M --> N
    N --> O[Merge into card agent_state under run key]
    O --> P[record_run row: trigger + agent + ok + summary max OUTPUT_PREVIEW_MAX]
    P --> Q[Move card to done or failed status]
    Q --> R[Return RunRecord to UI]
```

```mermaid
graph TD
    A[Scheduler tick - every TICK_SECS = 30] --> B[List all cards]
    B --> C[Keep cards with cron]
    C --> D{Cron parses?}
    D -->|no| E[Drop from schedule map]
    D -->|yes| F[Read / init next fire time under RwLock]
    F --> G{Due now?}
    G -->|no| H[Wait for next tick]
    G -->|yes| I[run_card - outside the lock]
    I -->|ok| J[Next fire = cron.next_after now]
    I -->|store error| K[Retry next tick - now + TICK_SECS]
    J --> H
    K --> H
    E --> H
```

A task card is the second way to drive an agent from the web. Cards carry
per-card agent state (`agent_name` + `agent_state` JSON) and an optional
sandbox `image`. There are **no pipelines anymore**: a card run submits the
card's task to its assigned agent's configured model — the agent's own tools
(shell, fetch, search, git, …) do the real work from a chat turn. Recurring
automation is **not** a card feature anymore — see
[Routines](#routines-owner-handled) below.

### Task statuses (`TaskStatus`, `crates/task-rs/src/card.rs`)

The board column IS the task status. Canonical enum:
`todo | in_progress | review | conflict | done | failed` (stored in
`column_id`; legacy `doing` parses as `in_progress`).

- A Board Card IS a Task — one entity (`task_cards` table), no separate
  card model. `POST /api/tasks` creates a task (status defaults `todo`).
- `PATCH /api/tasks/{id}/status` moves a task between statuses; the
  **backend** validates transitions (`transition_allowed`, service:
  `CardService::set_status`); illegal moves → 400. Allowed:
  todo→{in_progress,done,failed}; in_progress→{review,conflict,done,failed,
  todo}; review→{done,conflict,in_progress}; conflict→{review,in_progress,
  done}; done→{todo,in_progress}; failed→{todo,in_progress}.
- `/api/task/cards*` routes remain as backward-compatible aliases; card runs
  move through the same enum (start → in_progress, ok → done, fail →
  failed).

### Manual run (`POST /api/task/cards/{id}/run`)

Requires editor+ role. `app/card_run.rs::run_card`:

1. Load the card; missing id → `NoSuchCard`.
2. Move the card to the **doing** column (no-op if already there).
3. If no `agent_name` is assigned, or the agent config is missing, or no
   inference engine is configured, the run still **records a failure** (so the
   card moves on instead of silently staying put).
4. Otherwise build a prompt from the agent config (`persona:`, `instruction:`,
   `output format:` sections, when non-empty) + the card's `title` +
   `description`, and submit it to the engine with `INFER_MAX_TOKENS`.
5. `persist` merges the `RunRecord` into the card's `agent_state` under the
   `"run"` key (ledger only — the pinned `agent_name` preference is never
   mutated by a run), appends a `run_records` row (trigger, agent, ok,
   summary truncated to `OUTPUT_PREVIEW_MAX` chars), and moves the card to
   **done** (ok) or **failed**. Run history: `GET /api/activity` and the
   card's `agent_state`.
6. The `RunRecord` (agent, status, output, finished_at) is returned to the UI.

### Scheduled runs — Routines (`app/schedule_work.rs` + `app/routine_run.rs`)

Routines are a separate entity from tasks (own tables `routines` +
`routine_runs`; a routine never touches the board). The scheduler ticks
every `TICK_SECS = 30` and scans **enabled routines only** — card cron is
no longer scheduled (`card.cron` column remains in the DB, unscheduled).

- Due routine → `routine_run::run_routine`: builds a prompt from the named
  agent's config (persona/prompt/output format, when set) + the routine's
  `instruction`, submits to the engine (agent model routed via `engines`),
  and stores a `routine_runs` row (trigger `cron`, ok, summary). Next fire
  = `cron.next_after(now)`; store error → retry next tick; unparseable
  cron → dropped from the schedule.
- API: `GET/POST /api/routines`, `PUT/DELETE /api/routines/{id}`,
  `POST /api/routines/{id}/run` — **owner role only** (`require_owner`;
  routine runs are owner-handled). `GET /api/routines/{id}/runs` is
  readable by any role. Cron format is validated at create/update.
- UI: `/routines` page — compact list (cron label, agent, paused badge),
  run-now, runs history, create/edit with the cron builder. Task cards no
  longer carry a schedule editor.
- `GET /api/cronjobs` still exposes the schedule handle entries.

### Chat grounding: task context snapshot

A chat turn with `card_id` set gets a compact **TASK CONTEXT** snapshot
prepended (`BoardOps::card_context`): id, title, status, priority,
description and the last 20 comments. Comment threads on a task are the
durable context store — an `@agent` mention in a task comment sends the
chat turn **as that named agent** (persona, tool allow-list, thinking
level) with `card_id` attached, and posts the reply back as a comment.

### Card agent state API

- `GET /api/task/cards/{id}/agent` — read the card's agent state
  (includes the last `run` record after a run).
- `PUT /api/task/cards/{id}/agent` — set `agent_name` + arbitrary JSON
  `state` (editor+ role).

## Key constants

| Constant | Value | Meaning |
|---|---|---|
| `TOOL_ROUNDS_MAX` (prompt.rs) | 8 | max tool rounds before forced final answer |
| `CONTEXT_RESULTS_MAX` (prompt.rs) | 5 | search results fed into a prompt |
| `ARTIFACT_SCAN_MAX` (chat.rs) | 8 | files picked up from one shell output |
| `RESOURCE_IMAGE_MAX_BYTES` (chat.rs) | 4 MiB | larger images linked by path, not inlined |
| `RESOURCE_TEXT_MAX_CHARS` (chat.rs) | 64 Ki | text resource truncation |
| `INFER_MAX_TOKENS` (card_run.rs) | 1024 | card-run generation cap |
| `OUTPUT_PREVIEW_MAX` (card_run.rs) | 400 | run summary chars stored in `run_records` |
| `TICK_SECS` (schedule_work.rs) | 30 | scheduler poll interval |
