import { useEffect, useRef, useState } from "react";
import { NavLink, useNavigate } from "react-router-dom";
import {
  Bot,
  Check,
  CircleUserRound,
  Clock,
  KanbanSquare,
  FolderOpen,
  KeyRound,
  LogOut,
  MessageSquareText,
  Plus,
  Settings,
  Workflow,
} from "lucide-react";
import { use_workspaces } from "./WorkspaceContext.tsx";
import {
  change_password,
  clear_token,
  create_project,
  create_workspace,
  get_token,
  logout,
} from "../lib.js";
import { Modal, PromptModal } from "../ui/Overlay.js";
import { toast } from "../ui/Toast.js";
import { use_projects } from "./ProjectContext.tsx";

const ITEMS = [
  { to: "/chat", title: "chat", Icon: MessageSquareText },
  { to: "/kanban", title: "kanban", Icon: KanbanSquare },
  { to: "/pipelines", title: "pipelines", Icon: Workflow },
  { to: "/cronjobs", title: "cronjobs", Icon: Clock },
  { to: "/agents", title: "agents", Icon: Bot },
  { to: "/settings", title: "settings", Icon: Settings },
];

const MIN_PASSWORD_LEN = 8;


export default function SideNav() {
  const nav = useNavigate();
  const { workspaces, ws_id, pick, reload } = use_workspaces();
  const { projects, project_id, pick_project, reload_projects } = use_projects();
  const [creating, set_creating] = useState<null | "workspace" | "project">(null);
  const [menu_open, set_menu_open] = useState(false);
  const [pw_open, set_pw_open] = useState(false);
  const [ws_open, set_ws_open] = useState(false);

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
