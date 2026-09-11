import { useEffect, useState } from "react";
import { RefreshCw, TerminalSquare, Unplug } from "lucide-react";
import {
  fetch_agent_logs,
  fetch_machines,
  fetch_machine_agents,
  fetch_sandbox_logs,
  fetch_users,
  kick_machine,
  run_agent_command,
  run_machine_agent,
  type AgentLogs,
  type MachineAgent,
  type MachineView,
  type SandboxLogEntry,
  type UserInfo,
} from "../lib.js";
import { Modal } from "../ui/Overlay.js";
import { toast } from "../ui/Toast.js";

const POLL_TICK_MS = 5000;

const SANDBOX_NAME_PREFIX = "susutaku-agent-sandbox-";

const INSPECT_PRESETS = ["pwd", "ls -la"];

const KICK_CONFIRM_RESET_MS = 3000;

interface InspectEntry {
  cmd: string;
  output: string;
}

export function AgentInspect({ agent }: { agent: string }) {
  const [cmd, set_cmd] = useState("");
  const [busy, set_busy] = useState(false);
  const [entries, set_entries] = useState<InspectEntry[]>([]);

  async function run(raw: string) {
    const c = raw.trim();
    if (!c || busy) return;
    set_busy(true);
    try {
      const output = await run_agent_command(agent, c);
      set_entries((prev) => [...prev, { cmd: c, output }]);
      set_cmd("");
    } catch (err: unknown) {
      toast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      set_busy(false);
    }
  }

  return (
    <div className="agent-inspect">
      <div className="agent-inspect-row">
        {INSPECT_PRESETS.map((p) => (
          <button key={p} className="agent-inspect-preset" disabled={busy} onClick={() => run(p)}>
            {p}
          </button>
        ))}
      </div>
      <form
        className="agent-inspect-row"
        onSubmit={(e) => {
          e.preventDefault();
          void run(cmd);
        }}
      >
        <input
          value={cmd}
          disabled={busy}
          placeholder="run a command…"
          onChange={(e) => set_cmd(e.target.value)}
        />
        <button type="submit" className="agent-inspect-run" disabled={busy || !cmd.trim()} title="run">
          <TerminalSquare size={14} />
        </button>
      </form>
      {entries.map((e, i) => (
        <pre key={i} className="agent-inspect-out">{`$ ${e.cmd}\n${e.output}`}</pre>
      ))}
    </div>
  );
}

function KickButton({ hostname, on_done }: { hostname: string; on_done: () => void }) {
  const [armed, set_armed] = useState(false);
  const [busy, set_busy] = useState(false);

  async function kick() {
    if (!armed) {
      set_armed(true);
      setTimeout(() => set_armed(false), KICK_CONFIRM_RESET_MS);
      return;
    }
    set_busy(true);
    try {
      const { kicked } = await kick_machine(hostname);
      toast(kicked ? `disconnected ${hostname}` : `${hostname} was not connected`, kicked ? "success" : "info");
      on_done();
    } catch (err: unknown) {
      toast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      set_busy(false);
      set_armed(false);
    }
  }

  return (
    <button
      className={`machine-kick ${armed ? "armed" : ""}`}
      disabled={busy}
      onClick={(e) => {
        e.stopPropagation();
        void kick();
      }}
      title={armed ? "click again to confirm" : `disconnect ${hostname} from the hub`}
      aria-label={`disconnect ${hostname}`}
    >
      {armed ? "confirm?" : <Unplug size={13} />}
    </button>
  );
}

function MachineRow({ m, on_kick_done }: { m: MachineView; on_kick_done: () => void }) {
  return (
    <div className="dock-machine-row" role="presentation">
      <span className={`dot ${m.ok ? "alive" : "stale"}`} aria-hidden="true" />
      <span className="dock-machine-name">{m.hostname}</span>
      <small>
        {m.os}{m.arch ? `/${m.arch}` : ""}{m.ram_gib > 0 ? ` · ${m.ram_gib} GiB` : ""}
        {" · "}{m.ok ? "ok" : "unreachable"} — {machine_role(m)} · {agents_label(m)}
      </small>
      {m.client_id !== null && <KickButton hostname={m.hostname} on_done={on_kick_done} />}
    </div>
  );
}

function MachineLogs({ agent }: { agent: string }) {
  const [logs, set_logs] = useState<AgentLogs | null>(null);
  const [error, set_error] = useState("");

  useEffect(() => {
    set_logs(null);
    set_error("");
    fetch_agent_logs(agent)
      .then(set_logs)
      .catch((err: unknown) => set_error(err instanceof Error ? err.message : String(err)));
  }, [agent]);

  return (
    <div className="agent-logs">
      {error && <p className="error">{error}</p>}
      {!error && !logs && <p className="empty">loading…</p>}
      {logs && (
        <>
          <div className="agent-logs-meta">
            <span className="agent-logs-label">agent {logs.agent} on {logs.machine}</span>
            <code className="dock-agent-path" title={logs.work_tree}>{logs.work_tree}</code>
            <span className="agent-logs-label">{logs.runs} runs</span>
          </div>
          <AgentInspect agent={agent} />
          <p className="agent-logs-result">last result: {logs.last_result ?? "—"}</p>
          {logs.transcript.length === 0 ? (
            <p className="empty">no transcript yet</p>
          ) : (
            <pre className="agent-logs-pre">{logs.transcript.join("\n")}</pre>
          )}
        </>
      )}
    </div>
  );
}

function SandboxLogs({ path }: { path: string }) {
  const [logs, set_logs] = useState<SandboxLogEntry[] | null>(null);
  const [error, set_error] = useState("");

  useEffect(() => {
    // Poll while selected: the transcript grows as the agent spawns and
    // runs commands, so the panel streams without manual refresh.
    let alive = true;
    const tick = () => {
      fetch_sandbox_logs(path)
        .then((l) => {
          if (!alive) return;
          set_logs(l);
          set_error("");
        })
        .catch((err: unknown) => {
          if (alive && logs === null) {
            set_error(err instanceof Error ? err.message : String(err));
          }
        });
    };
    tick();
    const t = setInterval(tick, POLL_TICK_MS);
    return () => {
      alive = false;
      clearInterval(t);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path]);

  return (
    <>
      {error && <p className="error">{error}</p>}
      {!error && !logs && <p className="empty">loading…</p>}
      {logs && (logs.length === 0 ? (
        <p className="empty">no transcript yet — commands the agent runs will appear here</p>
      ) : (
        <pre className="agent-logs-pre">{logs.map((e) => e.content).join("\n")}</pre>
      ))}
    </>
  );
}

function MachineAgentConsole({ hostname }: { hostname: string }) {
  const [agent, set_agent] = useState("");
  const [cmd, set_cmd] = useState("");
  const [busy, set_busy] = useState(false);
  const [entries, set_entries] = useState<InspectEntry[]>([]);

  async function run() {
    const a = agent.trim();
    const c = cmd.trim();
    if (!a || !c || busy) return;
    set_busy(true);
    try {
      const { output } = await run_machine_agent(hostname, a, c);
      set_entries((prev) => [...prev, { cmd: `${a}$ ${c}`, output }]);
      set_cmd("");
    } catch (err: unknown) {
      toast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      set_busy(false);
    }
  }

  return (
    <div className="agent-logs">
      <div className="agent-logs-meta">
        <span className="dot remote" aria-hidden="true" />
        <span className="agent-logs-label">agents on {hostname}</span>
      </div>
      <form
        className="agent-inspect-row"
        onSubmit={(e) => {
          e.preventDefault();
          run();
        }}
      >
        <input
          value={agent}
          disabled={busy}
          placeholder="agent name…"
          onChange={(e) => set_agent(e.target.value)}
        />
      </form>
      <form
        className="agent-inspect-row"
        onSubmit={(e) => {
          e.preventDefault();
          run();
        }}
      >
        <input
          value={cmd}
          disabled={busy || !agent.trim()}
          placeholder="run a command in the agent sandbox…"
          onChange={(e) => set_cmd(e.target.value)}
        />
        <button type="submit" className="agent-inspect-run" disabled={busy || !agent.trim() || !cmd.trim()} title="run">
          <TerminalSquare size={14} />
        </button>
      </form>
      {entries.map((e, i) => (
        <pre key={i} className="agent-inspect-out">{`$ ${e.cmd}\n${e.output}`}</pre>
      ))}
    </div>
  );
}

type Props = {
  open: boolean;
  on_close: () => void;
};

type Selected =
  | { kind: "agent"; name: string }
  | { kind: "sandbox"; machine: string; path: string; pid: number; alive: boolean; local: boolean }
  | { kind: "machine"; hostname: string };

type Tab = "users_machines" | "agents_sandbox";

const TABS: { id: Tab; label: string }[] = [
  { id: "users_machines", label: "users & hosts" },
  { id: "agents_sandbox", label: "agent sandboxes" },
];

function machine_role(m: MachineView): string {
  const role = m.role ? `${m.role} host` : "registered host";
  if (m.local) return `this machine — ${role} (backend + agent sandboxes)`;
  if (m.client_id !== null) return `${role} — runs agent sandboxes for this server`;
  return role;
}

function agents_label(m: MachineView): string {
  if (m.agents.length === 0) return "no agents";
  return `agents: ${m.agents.join(", ")}`;
}

export function MachinesModal({ open, on_close }: Props) {
  const [tab, set_tab] = useState<Tab>("users_machines");
  const [agents, set_agents] = useState<MachineAgent[] | null>(null);
  const [machines, set_machines] = useState<MachineView[] | null>(null);
  const [users, set_users] = useState<UserInfo[] | null>(null);
  const [selected, set_selected] = useState<Selected | null>(null);

  async function load() {
    const [a, m, u] = await Promise.all([
      fetch_machine_agents().catch(() => []),
      fetch_machines().catch(() => []),
      fetch_users().catch(() => []),
    ]);
    set_agents(a);
    set_machines(m);
    set_users(u);
  }

  function switch_tab(t: Tab) {
    set_tab(t);
    set_selected(null);
  }

  useEffect(() => {
    if (!open) return;
    void load();
    const tick = setInterval(load, POLL_TICK_MS);
    return () => clearInterval(tick);
  }, [open]);

  return (
    <Modal open={open} title="machines" on_close={on_close} wide className="machines-modal">
      <div className="machines-tabs" role="tablist" aria-label="machine views">
        {TABS.map((t) => (
          <button
            key={t.id}
            role="tab"
            aria-selected={tab === t.id}
            className={`machines-tab ${tab === t.id ? "active" : ""}`}
            onClick={() => switch_tab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div className="machines-layout">
        <section className="machines-register" aria-label="machine register">
          <div className="dock-machines-head">
            <span>{tab === "users_machines" ? "users" : "sandboxes by host"}</span>
            {tab === "agents_sandbox" && machines !== null && (
              <small className="dock-machines-count">
                {machines.length} machine{machines.length === 1 ? "" : "s"}
              </small>
            )}
            <button onClick={() => void load()} title="refresh" aria-label="refresh machines">
              <RefreshCw size={13} />
            </button>
          </div>
          {tab === "users_machines" && (
            <div className="dock-machines-list">
              {users === null && machines === null && (
                <span className="dock-ws-empty">loading…</span>
              )}
              {users?.map((u) => (
                <div key={u.username} className="machine-group">
                  <div className="machine-group-head" role="presentation">
                    <span className="dot kind-agent" aria-hidden="true" />
                    <span className="dock-machine-name">{u.username}</span>
                    <small>{u.role}</small>
                  </div>
                </div>
              ))}
              {machines !== null && machines.length > 0 && (
                <div className="machine-group">
                  <div className="machine-group-head" role="presentation">
                    <span className="dot remote" aria-hidden="true" />
                    <span className="dock-machine-name">machines</span>
                  </div>
                  {machines.map((m) => <MachineRow key={m.hostname} m={m} on_kick_done={() => void load()} />)}
                </div>
              )}
            </div>
          )}
          {tab === "agents_sandbox" && (
            <div className="dock-machines-list">
              {machines === null && <span className="dock-ws-empty">loading…</span>}
              {machines !== null && machines.every((m) => m.sandboxes.length === 0) && (
                <span className="dock-ws-empty">no sandboxes</span>
              )}
              {machines?.filter((m) => m.sandboxes.length > 0).map((m) => (
                <div key={m.hostname} className="machine-group">
                  <div className="machine-group-head" role="presentation">
                    <span className={`dot ${m.ok ? "alive" : "stale"}`} aria-hidden="true" />
                    <span className="dock-machine-name">{m.hostname}</span>
                    <small>
                      {m.os}{m.arch ? `/${m.arch}` : ""}{m.ram_gib > 0 ? ` · ${m.ram_gib} GiB` : ""}
                      {" · "}{m.ok ? "ok" : "unreachable"} — {agents_label(m)}
                      {" · "}{m.sandboxes.length} sandbox{m.sandboxes.length === 1 ? "" : "es"}
                    </small>
                    {m.client_id !== null && <KickButton hostname={m.hostname} on_done={() => void load()} />}
                  </div>
                  {m.sandboxes.map((s) => {
                    const active = selected?.kind === "sandbox" && selected.path === s.path;
                    const short = (s.path.split("/").pop() || s.path).replace(SANDBOX_NAME_PREFIX, "");
                    return (
                      <button
                        key={s.path}
                        className={`dock-machine-row machine-sandbox ${active ? "selected" : ""}`}
                        onClick={() => set_selected({ kind: "sandbox", machine: m.hostname, path: s.path, pid: s.pid, alive: s.alive, local: m.local })}
                        title={`workspace sandbox on ${m.hostname} — ${s.path}`}
                      >
                        <span className={`dot ${s.alive ? "alive" : "stale"}`} aria-hidden="true" />
                        <span className="dock-machine-name" title={s.path}>agent sandbox {short.slice(0, 8)}</span>
                        <small>{s.alive ? "live — agent running" : "stale"}</small>
                      </button>
                    );
                  })}
                </div>
              ))}
              {machines?.some((m) => m.client_id !== null) && (
                <div className="machine-group">
                  <div className="machine-group-head" role="presentation">
                    <span className="dot remote" aria-hidden="true" />
                    <span className="dock-machine-name">shared machines</span>
                  </div>
                  {machines?.filter((m) => m.client_id !== null).map((m) => (
                    <div key={m.hostname} className="dock-agent-row">
                      <button
                        className={`dock-machine-row machine-sandbox ${selected?.kind === "machine" && selected.hostname === m.hostname ? "selected" : ""}`}
                        onClick={() => set_selected({ kind: "machine", hostname: m.hostname })}
                        title={`spawn agents on ${m.hostname}`}
                      >
                        <span className="dot remote" aria-hidden="true" />
                        <span className="dock-machine-name">{m.hostname}</span>
                        <small>spawn agent</small>
                      </button>
                      <KickButton hostname={m.hostname} on_done={() => void load()} />
                    </div>
                  ))}
                </div>
              )}
              {agents !== null && agents.length > 0 && (
                <div className="machine-group">
                  <div className="machine-group-head" role="presentation">
                    <span className="dot kind-agent" aria-hidden="true" />
                    <span className="dock-machine-name">agents</span>
                  </div>
                  {agents.map((m) => (
                    <div key={m.agent} className="dock-agent-row">
                      <button
                        className={`dock-machine-row machine-sandbox ${selected?.kind === "agent" && selected.name === m.agent ? "selected" : ""}`}
                        onClick={() => set_selected({ kind: "agent", name: m.agent })}
                        title={`logs — ${m.agent}`}
                      >
                        <span className="dot alive" aria-hidden="true" />
                        <span className="dock-machine-name">{m.agent}</span>
                        <small>{m.runs} runs</small>
                      </button>
                      <code className="dock-agent-path" title={m.work_tree}>{m.work_tree}</code>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </section>
        <section className="machines-logs" aria-label="machine logs">
          {selected === null && <p className="empty">select a machine…</p>}
          {selected?.kind === "agent" && <MachineLogs agent={selected.name} />}
          {selected?.kind === "sandbox" && (
            <div className="agent-logs">
              <div className="agent-logs-meta">
                <span className={`dot ${selected.alive ? "alive" : "stale"}`} aria-hidden="true" />
                <span className="agent-logs-label">agent sandbox on {selected.machine} (pid {selected.pid})</span>
              </div>
              <code className="dock-agent-path" title={selected.path}>{selected.path}</code>
              <p className="agent-logs-result">
                {selected.alive
                  ? "live — a running agent works in this sandbox; its commands stream below"
                  : "stale — the agent process that owned this sandbox is gone"}
              </p>
              {selected.local === false ? (
                <p className="empty">no logs — remote sandbox, logs stay on the client machine</p>
              ) : (
                <SandboxLogs path={selected.path} />
              )}
            </div>
          )}
          {selected?.kind === "machine" && <MachineAgentConsole hostname={selected.hostname} />}
        </section>
      </div>
    </Modal>
  );
}
