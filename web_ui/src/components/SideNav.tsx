import { useEffect, useRef, useState } from "react";
import { NavLink, useNavigate } from "react-router-dom";
import {
  Activity,
  Bot,
  Check,
  CircleUserRound,
  Clock,
  KanbanSquare,
  FolderOpen,
  KeyRound,
  LogOut,
  Monitor,
  Server,
  Plus,
  RefreshCw,
  Settings,
  TerminalSquare,
  Workflow,
} from "lucide-react";
import { use_workspaces } from "./WorkspaceContext.tsx";
import {
  change_password,
  clear_token,
  create_project,
  create_workspace,
  fetch_agent_logs,
  fetch_activity,
  fetch_host_spec,
  fetch_machine_agents,
  fetch_sandboxes,
  get_token,
  logout,
  type AgentLogs,
  type HostSpec,
  type MachineAgent,
  type SandboxDir,
  type ActivityEntry,
  gib_label,
  run_agent_command,
} from "../lib.js";
import { Modal, PromptModal } from "../ui/Overlay.js";
import { toast } from "../ui/Toast.js";
import { use_projects } from "./ProjectContext.tsx";

const ITEMS = [
  { to: "/kanban", title: "kanban", Icon: KanbanSquare },
  { to: "/pipelines", title: "pipelines", Icon: Workflow },
  { to: "/cronjobs", title: "cronjobs", Icon: Clock },
  { to: "/agents", title: "agents", Icon: Bot },
  { to: "/settings", title: "settings", Icon: Settings },
];

const MIN_PASSWORD_LEN = 8;

const KINDS = ["card", "pipeline", "agent", "run", "user", "workspace", "project"] as const;

type Kind = (typeof KINDS)[number];

function as_kind(kind: string): Kind {
  return (KINDS as readonly string[]).includes(kind) ? (kind as Kind) : "run";
}

function activity_time(created_at: string): string {
  const d = new Date(created_at);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  const time = `${hh}:${mm}:${ss}`;
  const now = new Date();
  if (d.toDateString() === now.toDateString()) return time;
  return `${d.getMonth() + 1}/${d.getDate()} ${time}`;
}

interface InspectEntry {
  cmd: string;
  output: string;
}

const INSPECT_PRESETS = ["pwd", "ls -la"];

function AgentInspect({ agent }: { agent: string }) {
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

function AgentLogsModal({ agent, on_close }: { agent: string | null; on_close: () => void }) {
  const [logs, set_logs] = useState<AgentLogs | null>(null);
  const [error, set_error] = useState("");

  useEffect(() => {
    if (!agent) return;
    set_logs(null);
    set_error("");
    fetch_agent_logs(agent)
      .then(set_logs)
      .catch((err: unknown) => set_error(err instanceof Error ? err.message : String(err)));
  }, [agent]);

  return (
    <Modal open={agent != null} title={`logs — ${agent ?? ""}`} on_close={on_close} wide>
      {error && <p className="error">{error}</p>}
      {!error && !logs && <p className="empty">loading…</p>}
      {logs && agent != null && (
        <div className="agent-logs">
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
        </div>
      )}
    </Modal>
  );
}


export default function SideNav() {
  const nav = useNavigate();
  const { workspaces, ws_id, pick, reload } = use_workspaces();
  const { projects, project_id, pick_project, reload_projects } = use_projects();
  const [creating, set_creating] = useState<null | "workspace" | "project">(null);
  const [menu_open, set_menu_open] = useState(false);
  const [pw_open, set_pw_open] = useState(false);
  const [ws_open, set_ws_open] = useState(false);
  const [machines_open, set_machines_open] = useState(false);
  const [agents, set_agents] = useState<MachineAgent[] | null>(null);
  const [host, set_host] = useState<HostSpec | null>(null);
  const [sandboxes, set_sandboxes] = useState<SandboxDir[]>([]);
  const [logs_agent, set_logs_agent] = useState<string | null>(null);
  const [activity_open, set_activity_open] = useState(false);
  const [activity, set_activity] = useState<ActivityEntry[] | null>(null);
  const [activity_error, set_activity_error] = useState(false);

  const menu_ref = useRef<HTMLDivElement | null>(null);


  useEffect(() => {
    if (!menu_open) return;
    const on_doc_click = (e: MouseEvent) => {
      if (menu_ref.current && !menu_ref.current.contains(e.target as Node)) set_menu_open(false);
    };
    document.addEventListener("mousedown", on_doc_click);
    return () => document.removeEventListener("mousedown", on_doc_click);
  }, [menu_open]);

  if (!get_token()) return null;

  async function do_logout() {
    set_menu_open(false);
    await logout();
    nav("/", { replace: true });
  }

  async function submit_create(name: string) {
    if (!creating) return;
    try {
      if (creating === "workspace") {
        const res = await create_workspace(name);
        if (!res.ok) throw new Error(await res.text());
        toast(`workspace "${name}" created`, "success");
        await reload();
      } else if (ws_id != null) {
        const res = await create_project(ws_id, name);
        if (!res.ok) throw new Error(await res.text());
        toast(`project "${name}" created`, "success");
        await reload_projects();
      }
    } catch (err: unknown) {
      toast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      set_creating(null);
    }
  }

  async function load_machines() {
    set_agents(null);
    set_sandboxes([]);
    set_host(null);
    const [a, s, h] = await Promise.all([
      fetch_machine_agents().catch(() => []),
      fetch_sandboxes().catch(() => []),
      fetch_host_spec().catch(() => null),
    ]);
    set_agents(a);
    set_sandboxes(s);
    set_host(h);
  }

  function toggle_machines() {
    set_machines_open((v) => {
      if (!v) load_machines();
      return !v;
    });
  }

  async function load_activity() {
    set_activity(null);
    set_activity_error(false);
    try {
      set_activity(await fetch_activity());
    } catch {
      set_activity_error(true);
    }
  }

  function toggle_activity() {
    set_activity_open((v) => {
      if (!v) load_activity();
      return !v;
    });
  }

  return (
    <nav className="dock-nav" aria-label="main navigation">
      {ITEMS.map(({ to, title, Icon }) => (
        <NavLink
          key={to}
          to={to}
          aria-label={title}
          className={({ isActive }) => (isActive ? "active" : "")}
        >
          <Icon size={16} />
          <span className="dock-label">{title}</span>
        </NavLink>
      ))}
      <div className="dock-scope">
        <select
          className="dock-select"
          value={ws_id ?? ""}
          onChange={(e: React.ChangeEvent<HTMLSelectElement>) => pick(Number(e.target.value))}
          title="workspace"
          aria-label="workspace"
        >
          {workspaces.length === 0 && <option value="">no workspace</option>}
          {workspaces.map((w) => (
            <option key={w.id} value={w.id}>{w.name}</option>
          ))}
        </select>
        <button
          className="dock-mini"
          onClick={() => set_creating("workspace")}
          title="new workspace"
          aria-label="new workspace"
        >+</button>
        <span className="dock-divider" aria-hidden="true" />
        <select
          className="dock-select"
          value={project_id ?? ""}
          onChange={(e: React.ChangeEvent<HTMLSelectElement>) => pick_project(e.target.value ? Number(e.target.value) : null)}
          title="project"
          aria-label="project"
        >
          {projects.length === 0 && <option value="">no project</option>}
          {projects.map((p) => (
            <option key={p.id} value={p.id}>{p.name}</option>
          ))}
        </select>
        <button
          className="dock-mini"
          onClick={() => set_creating("project")}
          disabled={ws_id == null}
          title="new project"
          aria-label="new project"
        >+</button>
      </div>
      <div className="dock-profile">
        <button
          className={`dock-profile-btn ${machines_open ? "open" : ""}`}
          onClick={toggle_machines}
          title="machines"
          aria-haspopup="menu"
          aria-expanded={machines_open}
        >
          <Monitor size={16} />
          <span className="dock-label">machines</span>
        </button>
        {machines_open && (
          <div className="dock-profile-menu" role="menu" aria-label="machines menu">
            <div className="dock-machines-head">
              <span>machines</span>
              <button
                onClick={load_machines}
                title="refresh"
                aria-label="refresh machines"
              >
                <RefreshCw size={13} />
              </button>
            </div>
            <div className="dock-host">
              {host === null && <span className="dock-ws-empty">host specs unavailable</span>}
              {host && (
                <>
                  <div className="dock-machine-row" role="presentation">
                    <Server size={14} />
                    <span className="dock-machine-name">{host.hostname}</span>
                    <small>{host.os}/{host.arch}</small>
                  </div>
                  <div className="dock-host-specs">
                    {host.cpu_model} · {host.cpu_cores} cores · {gib_label(host.memory_bytes)} RAM
                  </div>
                </>
              )}
            </div>
            <div className="dock-machines-list">
              {agents === null && <span className="dock-ws-empty">loading…</span>}
              {agents !== null && agents.length === 0 && (
                <span className="dock-ws-empty">no machines running</span>
              )}
              {agents?.map((m) => (
                <div key={m.agent} className="dock-agent-row">
                  <button
                    role="menuitem"
                    className="dock-machine-row"
                    onClick={() => set_logs_agent(m.agent)}
                    title={`logs — ${m.agent}`}
                  >
                    <span className="dot alive" aria-hidden="true" />
                    <span className="dock-machine-name">{m.agent}</span>
                    <small>{m.runs} runs</small>
                  </button>
                  <code className="dock-agent-path" title={m.work_tree}>{m.work_tree}</code>
                </div>
              ))}
              {sandboxes.map((s) => (
                <div key={s.path} className="dock-machine-row" role="presentation">
                  <span className={`dot ${s.alive ? "alive" : "stale"}`} aria-hidden="true" />
                  <span className="dock-machine-name" title={s.path}>{s.path.split("/").pop() || s.path}</span>
                  <small>pid {s.pid}</small>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
      <div className="dock-profile">
        <button
          className={`dock-profile-btn ${activity_open ? "open" : ""}`}
          onClick={toggle_activity}
          title="activity"
          aria-haspopup="menu"
          aria-expanded={activity_open}
        >
          <Activity size={16} />
          <span className="dock-label">activity</span>
        </button>
        {activity_open && (
          <div className="dock-profile-menu" role="menu" aria-label="activity menu">
            <div className="dock-machines-head">
              <span>activity</span>
              <button onClick={load_activity} title="refresh" aria-label="refresh activity">
                <RefreshCw size={13} />
              </button>
            </div>
            <div className="dock-activity-list">
              {activity_error && <span className="dock-ws-empty">activity unavailable</span>}
              {!activity_error && activity === null && <span className="dock-ws-empty">loading…</span>}
              {!activity_error && activity?.length === 0 && <span className="dock-ws-empty">no activity yet</span>}
              {activity?.map((e) => (
                <div key={e.id} className="dock-machine-row" role="presentation">
                  <span className={`dot kind-${as_kind(e.kind)}`} aria-hidden="true" title={e.kind} />
                  <span className="dock-activity-time">{activity_time(e.created_at)}</span>
                  <span className="dock-activity-msg" title={e.message}>{e.message}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
      <div className="dock-profile" ref={menu_ref}>
        <button
          className={`dock-profile-btn ${menu_open ? "open" : ""}`}
          onClick={() => set_menu_open((v) => !v)}
          title="account"
          aria-haspopup="menu"
          aria-expanded={menu_open}
        >
          <CircleUserRound size={16} />
          <span className="dock-label">account</span>
        </button>
        {menu_open && (
          <div className="dock-profile-menu" role="menu" aria-label="account menu">
            <button role="menuitem" onClick={() => { set_menu_open(false); set_pw_open(true); }}>
              <KeyRound size={14} /> change password
            </button>
            <CreateWorkspaceItem
              on_done={() => {
                reload();
                set_ws_open(true);
              }}
            />
            <button role="menuitem" onClick={() => { set_menu_open(false); set_ws_open(true); }}>
              <FolderOpen size={14} /> select workspace
            </button>
            <button role="menuitem" className="danger" onClick={do_logout}>
              <LogOut size={14} /> logout
            </button>
          </div>
        )}
      </div>
      <WorkspaceModal open={ws_open} on_close={() => set_ws_open(false)} />
      <ChangePasswordModal open={pw_open} on_close={() => set_pw_open(false)} on_done={() => nav("/", { replace: true })} />
      <AgentLogsModal agent={logs_agent} on_close={() => set_logs_agent(null)} />
      <PromptModal
        open={creating != null}
        title={creating === "project" ? "new project" : "new workspace"}
        placeholder="name…"
        on_close={() => set_creating(null)}
        on_submit={(v: string) => submit_create(v)}
      />
    </nav>
  );
}

function WorkspaceModal({ open, on_close }: { open: boolean; on_close: () => void }) {
  const { workspaces, ws_id, pick } = use_workspaces();

  return (
    <Modal open={open} title="select workspace" on_close={on_close}>
      <div role="radiogroup" aria-label="workspaces" className="dock-ws-list">
        {workspaces.length === 0 && <span className="dock-ws-empty">no workspaces</span>}
        {workspaces.map((w) => (
          <button
            key={w.id}
            role="radio"
            aria-checked={w.id === ws_id}
            onClick={() => { pick(w.id); on_close(); }}
          >
            {w.id === ws_id ? <Check size={14} /> : <span className="dock-ws-spacer" />}
            {w.name}
          </button>
        ))}
      </div>
    </Modal>
  );
}

function CreateWorkspaceItem({ on_done }: { on_done: () => void | Promise<void> }) {
  const [open, set_open] = useState(false);
  const [name, set_name] = useState("");
  const [busy, set_busy] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed || busy) return;
    set_busy(true);
    try {
      const res = await create_workspace(trimmed);
      if (!res.ok) throw new Error(await res.text());
      toast(`workspace "${trimmed}" created`, "success");
      set_open(false);
      set_name("");
      on_done();
    } catch (err: unknown) {
      toast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      set_busy(false);
    }
  }

  return (
    <>
      <button role="menuitem" onClick={() => set_open(true)}>
        <Plus size={14} /> create workspace
      </button>
      <Modal open={open} title="create workspace" on_close={() => set_open(false)}>
        <form className="modal-form" onSubmit={submit}>
          <input
            value={name}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => set_name(e.target.value)}
            placeholder="workspace name"
            autoFocus
            aria-label="workspace name"
          />
          <button type="submit" disabled={busy || !name.trim()}>
            create
          </button>
        </form>
      </Modal>
    </>
  );
}

function ChangePasswordModal({
  open,
  on_close,
  on_done,
}: {
  open: boolean;
  on_close: () => void;
  on_done: () => void;
}) {
  const [old_pw, set_old_pw] = useState("");
  const [new_pw, set_new_pw] = useState("");
  const [busy, set_busy] = useState(false);
  const [error, set_error] = useState("");

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    set_busy(true);
    set_error("");
    try {
      const res = await change_password(old_pw, new_pw);
      if (!res.ok) throw new Error(await res.text());
      toast("password changed — please login again", "success");
      on_close();
      clear_token();
      on_done();
    } catch (err: unknown) {
      set_error(err instanceof Error ? err.message : String(err));
    } finally {
      set_busy(false);
    }
  }

  return (
    <Modal open={open} title="change password" on_close={on_close}>
      <form className="modal-form" onSubmit={submit}>
        <input
          type="password"
          value={old_pw}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => set_old_pw(e.target.value)}
          placeholder="current password"
          autoComplete="current-password"
        />
        <input
          type="password"
          value={new_pw}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => set_new_pw(e.target.value)}
          placeholder={`new password (min ${MIN_PASSWORD_LEN} chars)`}
          autoComplete="new-password"
        />
        {error && <p className="error">{error}</p>}
        <button type="submit" disabled={busy || !old_pw || new_pw.length < MIN_PASSWORD_LEN}>
          change password
        </button>
      </form>
    </Modal>
  );
}
