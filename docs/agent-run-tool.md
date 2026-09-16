# AGENT_RUN — chat tool for driving a named agent's sandbox

## What it does

`TOOL: AGENT_RUN <agent> <cmd>` runs one shell command **as a named manager
agent, inside that agent's own podman sandbox** (`work/agents/<agent>/sandbox`
mounted at `/workspace`), spawning the agent on demand. The command output is
fed back into the chat as tool results, the command joins the agent's
transcript, and the slot's run counter advances — so the work shows up on the
review page (`/review`) exactly like a run from the agents page.

This closes the gap where a chat agent could only *talk about* work (printing
bash blocks for the user to paste) while the review page inspected a different,
untouched work tree.

## Chat tool surface

- Line protocol: `TOOL: AGENT_RUN zai cargo test --test multi_chat_test`
- XML protocol: `<invoke name="agent_run"><parameter name="agent">zai</parameter><parameter name="command">cargo test</parameter></invoke>`
- Permission kind: `agent` (per-agent allow-list in agent settings;
  empty allow-list = allowed like every tool).

## Implementation map

| Piece | File |
|---|---|
| Tool parse (line + XML) | `backend/src/domain/service/tool_call.rs` (`ToolCall::AgentRun`, `TOOL_AGENT_RUN`, `XML_AGENT_RUN` → reuses the line parser) |
| Permission kind `agent` | `backend/src/domain/valueobject/tool_set.rs` (`ToolKind::Agent`) |
| Prompt section | `backend/src/domain/service/prompt.rs` (`TOOL_AGENT_RUN_INSTRUCTION`, `Section::Agent`) |
| Execution | `backend/src/app/chat.rs` (`with_agent_run`, `ToolCall::AgentRun` arm, `spawn_blocking`) |
| Port | `backend/src/port/outbound/agent_run.rs` (`trait AgentRun::run_for(agent, cmd)`) |
| Adapter | `backend/src/infra/manager_run.rs` (`ManagerRun` → `Manager::run`) |
| Wiring | `backend/src/main.rs` (`.with_agent_run(...)` on `ChatUseCase`, used by `/api/chat`) + `backend/src/api.rs` `zai_chat_deps` (`/api/chat/zai` and `/api/chat/zai/stream`) |

## Notes & limits

- Commands run sequentially inside the agent slot; long builds block that
  agent's slot (the manager serializes per agent), not the whole backend
  (the call is wrapped in `spawn_blocking`).
- The tool is disabled (tool result is the error string `no agent runner is
  configured`) when the backend runs without a manager adapter — e.g. minimal
  chat-only deployments.
- Only the named agent's slot is touched; the chat's shared sandbox
  (`AgentSandbox::restore()`, `infra/podman.rs`) is unchanged, so
  `TOOL: SHELL` keeps its old semantics.
