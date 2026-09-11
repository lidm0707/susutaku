import { useEffect, useState } from "react";
import { RefreshCw, TerminalSquare } from "lucide-react";
import {
  fetch_agent_logs,
  fetch_machines,
  fetch_machine_agents,
  fetch_users,
  run_agent_command,
  type AgentLogs,
  type MachineAgent,
  type MachineView,
  type UserInfo,
} from "../lib.js";
import { Modal } from "../ui/Overlay.js";
import { toast } from "../ui/Toast.js";

const POLL_TICK_MS = 5000;

const UNKNOWN_USER = "unknown";

const INSPECT_PRESETS = ["pwd", "ls -la"];

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

function MachineRow({ m }: { m: MachineView }) {
  return (
    <div className="dock-machine-row" role="presentation">
      <span className={`dot ${m.local ? "alive" : "remote"}`} aria-hidden="true" />
      <span className="dock-machine-name">{m.hostname}</span>
      <small>{m.os}{m.arch ? `/${m.arch}` : ""}{m.local ? " · local" : ""}</small>
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
            <span className="agent-logs-label">work_tree</span>
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

type Props = {
  open: boolean;
  on_close: () => void;
};

type Selected =
  | { kind: "agent"; name: string }
  | { kind: "sandbox"; machine: string; path: string; pid: number; alive: boolean };

type Tab = "users_machines" | "agents_sandbox";

const TABS: { id: Tab; label: string }[] = [
  { id: "users_machines", label: "users + machines" },
  { id: "agents_sandbox", label: "agents + sandbox" },
];

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
            <span>{tab === "users_machines" ? "register" : "agents"}</span>
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
                  <small className="dock-ws-empty">no machines</small>
                </div>
              ))}
              {machines !== null && machines.length > 0 && (
                <div className="machine-group">
                  <div className="machine-group-head" role="presentation">
                    <span className="dot remote" aria-hidden="true" />
                    <span className="dock-machine-name">{UNKNOWN_USER}</span>
                  </div>
                  {machines.map((m) => <MachineRow key={m.hostname} m={m} />)}
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
                    <span className={`dot ${m.local ? "alive" : "remote"}`} aria-hidden="true" />
                    <span className="dock-machine-name">{m.hostname}</span>
                    <small>{m.os}{m.arch ? `/${m.arch}` : ""}{m.local ? " · local" : ""}</small>
                  </div>
                  {m.sandboxes.map((s) => {
                    const active = selected?.kind === "sandbox" && selected.path === s.path;
                    return (
                      <button
                        key={s.path}
                        className={`dock-machine-row machine-sandbox ${active ? "selected" : ""}`}
                        onClick={() => set_selected({ kind: "sandbox", machine: m.hostname, path: s.path, pid: s.pid, alive: s.alive })}
                        title={`sandbox on ${m.hostname} — ${s.path}`}
                      >
                        <span className={`dot ${s.alive ? "alive" : "stale"}`} aria-hidden="true" />
                        <span className="dock-machine-name" title={s.path}>{s.path.split("/").pop() || s.path}</span>
                        <small>{s.pid > 0 ? `pid ${s.pid}` : ""}</small>
                      </button>
                    );
                  })}
                </div>
              ))}
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
                <span className="agent-logs-label">sandbox on {selected.machine}</span>
              </div>
              <code className="dock-agent-path" title={selected.path}>{selected.path}</code>
              <p className="agent-logs-result">{selected.pid > 0 ? `pid ${selected.pid} · ` : ""}{selected.alive ? "alive" : "stale"}</p>
              <p className="empty">no logs — this is a workspace sandbox, not a spawned agent machine</p>
            </div>
          )}
        </section>
      </div>
    </Modal>
  );
}
