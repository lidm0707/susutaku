import { useEffect, useState } from "react";
import { NavLink, useNavigate } from "react-router-dom";
import {
  Activity,
  Bot,
  Check,
  CircleUserRound,
  Clock,
  KanbanSquare,
  KeyRound,
  LogOut,
  Monitor,
  Settings,
  Workflow,
  MessageSquareText,
  Gauge,
} from "lucide-react";
import { use_workspaces } from "./WorkspaceContext.tsx";
import {
  change_password,
  clear_token,
  create_project,
  create_workspace,
  fetch_agent_logs,
  get_token,
  logout,
  type AgentLogs,
} from "../lib.js";
import { ActivityModal } from "./ActivityModal.tsx";
import { Modal, PromptModal } from "../ui/Overlay.js";
import { AgentInspect, MachinesModal } from "./MachinesModal.tsx";
import { toast } from "../ui/Toast.js";
import { use_projects } from "./ProjectContext.tsx";
import QuotaBoard from "./QuotaBoard.tsx";

const ITEMS = [
  { to: "/kanban", title: "kanban", Icon: KanbanSquare },
  { to: "/pipelines", title: "pipelines", Icon: Workflow },
  { to: "/cronjobs", title: "cronjobs", Icon: Clock },
  { to: "/agents", title: "agents", Icon: Bot },
  { to: "/settings", title: "settings", Icon: Settings },
];

const MIN_PASSWORD_LEN = 8;

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


export default function SideNav({ on_chat }: { on_chat: () => void }) {
  const nav = useNavigate();
  const [profile_open, set_profile_open] = useState(false);
  const [pw_open, set_pw_open] = useState(false);
  const [machines_open, set_machines_open] = useState(false);
  const [logs_agent, set_logs_agent] = useState<string | null>(null);
  const [activity_open, set_activity_open] = useState(false);
  const [quota_open, set_quota_open] = useState(false);

  if (!get_token()) return null;

  async function do_logout() {
    set_profile_open(false);
    await logout();
    nav("/", { replace: true });
  }

  function toggle_machines() {
    set_machines_open((v) => !v);
  }

  function toggle_activity() {
    set_activity_open((v) => !v);
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
      <div className="dock-out">
        <button
          type="button"
          className={`chat-dock-btn quota-dock-btn ${quota_open ? "open" : ""}`}
          onClick={() => set_quota_open((v) => !v)}
          title="quota board"
          aria-label="toggle quota board"
          aria-expanded={quota_open}
        >
          <Gauge size={16} />
        </button>
        <button
          type="button"
          className="chat-dock-btn"
          onClick={on_chat}
          title="chat"
          aria-label="open chat"
        >
          <MessageSquareText size={16} />
        </button>
      </div>
      <div className="dock-tools">
        <div className="dock-profile">
          <button
            className={`dock-profile-btn ${machines_open ? "open" : ""}`}
            onClick={toggle_machines}
            title="machines"
            aria-haspopup="dialog"
            aria-expanded={machines_open}
          >
            <Monitor size={16} />
            <span className="dock-label">machines</span>
          </button>
        </div>
        <div className="dock-profile">
          <button
            className={`dock-profile-btn ${activity_open ? "open" : ""}`}
            onClick={toggle_activity}
            title="activity"
            aria-haspopup="dialog"
            aria-expanded={activity_open}
          >
            <Activity size={16} />
            <span className="dock-label">activity</span>
          </button>
        </div>
        <div className="dock-profile">
          <button
            className={`dock-profile-btn ${profile_open ? "open" : ""}`}
            onClick={() => set_profile_open(true)}
            title="profile"
            aria-haspopup="dialog"
            aria-expanded={profile_open}
          >
            <CircleUserRound size={16} />
            <span className="dock-label">profile</span>
          </button>
        </div>
      </div>
      <QuotaBoard open={quota_open} on_close={() => set_quota_open(false)} />
      <ProfileModal open={profile_open} on_close={() => set_profile_open(false)} on_logout={do_logout} on_change_password={() => { set_profile_open(false); set_pw_open(true); }} />
      <ChangePasswordModal open={pw_open} on_close={() => set_pw_open(false)} on_done={() => nav("/", { replace: true })} />
      <AgentLogsModal agent={logs_agent} on_close={() => set_logs_agent(null)} />
      <MachinesModal open={machines_open} on_close={() => set_machines_open(false)} />
      <ActivityModal open={activity_open} on_close={() => set_activity_open(false)} />
    </nav>
  );
}

function ProfileModal({
  open,
  on_close,
  on_logout,
  on_change_password,
}: {
  open: boolean;
  on_close: () => void;
  on_logout: () => void;
  on_change_password: () => void;
}) {
  const { workspaces, ws_id, pick } = use_workspaces();
  const { projects, project_id, pick_project } = use_projects();
  const [creating, set_creating] = useState<null | "workspace" | "project">(null);

  async function submit_create(name: string) {
    if (!creating) return;
    try {
      if (creating === "workspace") {
        const res = await create_workspace(name);
        if (!res.ok) throw new Error(await res.text());
        toast(`workspace "${name}" created`, "success");
      } else if (ws_id != null) {
        const res = await create_project(ws_id, name);
        if (!res.ok) throw new Error(await res.text());
        toast(`project "${name}" created`, "success");
      }
    } catch (err: unknown) {
      toast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      set_creating(null);
    }
  }

  return (
    <Modal open={open} title="profile" on_close={on_close}>
      <section className="profile-scope" aria-label="workspace">
        <header className="profile-scope-head">
          <span className="agent-logs-label">workspace</span>
          <button
            className="dock-mini"
            onClick={() => set_creating("workspace")}
            title="new workspace"
            aria-label="new workspace"
          >+</button>
        </header>
        <div role="radiogroup" aria-label="workspaces" className="dock-ws-list">
          {workspaces.length === 0 && <span className="dock-ws-empty">no workspaces</span>}
          {workspaces.map((w) => (
            <button
              key={w.id}
              role="radio"
              aria-checked={w.id === ws_id}
              onClick={() => pick(w.id)}
            >
              {w.id === ws_id ? <Check size={14} /> : <span className="dock-ws-spacer" />}
              {w.name}
            </button>
          ))}
        </div>
      </section>
      <section className="profile-scope" aria-label="project">
        <header className="profile-scope-head">
          <span className="agent-logs-label">project</span>
          <button
            className="dock-mini"
            onClick={() => set_creating("project")}
            disabled={ws_id == null}
            title="new project"
            aria-label="new project"
          >+</button>
        </header>
        <div role="radiogroup" aria-label="projects" className="dock-ws-list">
          {projects.length === 0 && <span className="dock-ws-empty">no projects</span>}
          {projects.map((p) => (
            <button
              key={p.id}
              role="radio"
              aria-checked={p.id === project_id}
              onClick={() => { pick_project(p.id); }}
            >
              {p.id === project_id ? <Check size={14} /> : <span className="dock-ws-spacer" />}
              {p.name}
            </button>
          ))}
        </div>
      </section>
      <section className="profile-actions" aria-label="account actions">
        <button onClick={on_change_password}>
          <KeyRound size={14} /> change password
        </button>
        <button className="danger" onClick={on_logout}>
          <LogOut size={14} /> logout
        </button>
      </section>
      <PromptModal
        open={creating != null}
        title={creating === "project" ? "new project" : "new workspace"}
        placeholder="name…"
        on_close={() => set_creating(null)}
        on_submit={(v: string) => submit_create(v)}
      />
    </Modal>
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
