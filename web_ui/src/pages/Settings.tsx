import { useEffect, useState } from "react";
import {
  KeyRound,
  Monitor,
  Cpu,
  Server,
  Globe,
  User,
  Terminal,
  Copy,
  Check,
  Zap,
  Code2,
  HardDrive,
  Pencil,
  Plus,
  Trash2,
} from "lucide-react";
import { Modal } from "../ui/Overlay.jsx";
import { SplitLayout, type SideItem } from "../ui/SplitLayout.jsx";
import CodexLogin from "../components/CodexLogin.tsx";
import ClaudeLogin from "../components/ClaudeLogin.tsx";
import {
  fetch_users,
  register_user,
  type UserInfo,
  fetch_zai_settings,
  zai_model_action,
  fetch_client_env,
  save_client_env,
  fetch_models,
  select_model,
  pretty_name,
  size_label,
  type ZaiSettings,
  type ZaiModel,
  type ZaiModelAction,
  type ClientEnv,
  type ModelInfo,
} from "../lib.js";

type Tab = "client" | "providers" | "users";

const ZAI_MODELS = ["glm-4.6", "glm-4.6v", "glm-4.5", "glm-4.5-air", "glm-4.5-flash", "glm-4.5v"] as const;

const TABS: { id: Tab; label: string }[] = [
  { id: "client", label: "client env" },
  { id: "providers", label: "ai providers" },
  { id: "users", label: "users" },
];

export default function Settings() {
  const [tab, setTab] = useState<Tab>("client");

  return (
    <main className="chat">
      <header>
        <h1>settings</h1>
      </header>
      <section className="log">
        <div className="settings-tabs" role="tablist">
          {TABS.map((t) => (
            <button
              key={t.id}
              role="tab"
              aria-selected={tab === t.id}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </div>
        {tab === "client" && <ClientEnvTab />}
        {tab === "providers" && <ProvidersTab />}
        {tab === "users" && <UsersTab />}
      </section>
    </main>
  );
}

/* --- client env --- */

const INSTALL_PATH = "/install.sh";

type InstallRole = "auto" | "model" | "worker";

const INSTALL_ROLES: { id: InstallRole; label: string; desc: string }[] = [
  { id: "auto", label: "auto", desc: "probe decides: model+worker when capable" },
  { id: "model", label: "model host", desc: "force local-model host (macos + aarch64 + ≥32 GiB)" },
  { id: "worker", label: "worker only", desc: "external provider jobs only (cloud api / codex / claude)" },
];

function install_command(role: InstallRole): string {
  const params = new URLSearchParams({ role, server: window.location.origin });
  return `curl -fsSL "${window.location.origin}${INSTALL_PATH}?${params}" | sh`;
}

function InstallCmd() {
  const [copied, setCopied] = useState(false);
  const [role, setRole] = useState<InstallRole>("auto");
  const cmd = install_command(role);
  async function copy() {
    try {
      await navigator.clipboard.writeText(cmd);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // clipboard unavailable (non-secure context) — command stays selectable
    }
  }
  return (
    <div className="install-cmd">
      <fieldset className="install-role-picker">
        <legend>installer type</legend>
        {INSTALL_ROLES.map((r) => (
          <label key={r.id} className="install-role">
            <input
              type="radio"
              name="install-role"
              checked={role === r.id}
              onChange={() => setRole(r.id)}
            />
            <span className="install-role-label">{r.label}</span>
            <span className="install-role-desc">{r.desc}</span>
          </label>
        ))}
      </fieldset>
      <div className="install-cmd-row">
        <code>{cmd}</code>
        <button type="button" className="icon-btn" onClick={copy} aria-label="copy install command">
          {copied ? <Check size={14} /> : <Copy size={14} />}
        </button>
      </div>
      <p className="install-cmd-hint">
        paste in a terminal on the other machine — it registers itself as a sandbox client
      </p>

      <h3 className="install-cmd-title">how it works</h3>
      <ol className="install-cmd-steps">
        <li>
          run the command above in a terminal on the machine you want to connect —
          it downloads the client and registers it as a sandbox client of this server
        </li>
        <li>
          the client creates its working dirs locally, named <code>susutaku-agent-sandbox-&lt;pid&gt;</code>
        </li>
        <li>
          once running, the machine shows up under <strong>sandbox</strong> in the sidebar
          — stale entries (dead clients) can be removed there anytime
        </li>
      </ol>

      <h3 className="install-cmd-title">to uninstall</h3>
      <ol className="install-cmd-steps">
        <li>
          stop the client on that machine: <code>pkill -f susutaku-agent-sandbox</code>
        </li>
        <li>
          remove its working dirs: <code>rm -rf ~/susutaku-agent-sandbox-*</code>
        </li>
        <li>
          on the server, open <strong>sandbox</strong> in the sidebar and press
          <strong> sweep stale</strong> to clean up the registration — or run
          <code> curl -X POST {window.location.origin}/api/sandbox/sweep</code>
        </li>
      </ol>
    </div>
  );
}

function ClientEnvTab() {
  const [env, setEnv] = useState<ClientEnv | null>(null);
  const [workspacePath, setWorkspacePath] = useState("");
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const [pane, setPane] = useState("host");

  useEffect(() => {
    fetch_client_env()
      .then((e: ClientEnv) => {
        setEnv(e);
        setWorkspacePath(e.workspace_path || "");
      })
      .catch((err: unknown) => setError(err instanceof Error ? err.message : String(err)));
  }, []);

  async function share(e: React.FormEvent) {
    e.preventDefault();
    setStatus("");
    setError("");
    try {
      const saved: ClientEnv = await save_client_env({
        user_agent: navigator.userAgent,
        platform: navigator.platform || "",
        language: navigator.language || "",
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || "",
        screen: `${window.screen.width}x${window.screen.height}`,
        workspace_path: workspacePath,
      });
      setEnv(saved);
      setStatus("shared with backend");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  const items: SideItem[] = [
    { id: "host", name: "host machine", desc: env?.hostname || "server", icon: <Server size={16} /> },
    ...(env?.reported
      ? [{ id: "browser", name: "browser client", desc: env.platform || "web", icon: <Globe size={16} /> }]
      : []),
    { id: "install", name: "install client", desc: "connect a computer", icon: <Terminal size={16} /> },
  ];

  return (
    <SplitLayout items={items} active={pane} on_pick={setPane} label="client environment">
      {pane === "host" && (
        <div className="env-share-pane">
          <section className="env-card env-card-wide" aria-label="host machine">
            <h3>
              <Server size={14} /> host machine
            </h3>
            <dl>
              <dt>hostname</dt>
              <dd>{env?.hostname || "—"}</dd>
              <dt>os</dt>
              <dd>{env?.os || "—"}</dd>
              <dt>arch</dt>
              <dd>{env?.arch || "—"}</dd>
              <dt>workspace path</dt>
              <dd>{env?.reported && env.workspace_path ? env.workspace_path : "—"}</dd>
            </dl>
          </section>
          <form className="settings-form env-share-form" onSubmit={share}>
            <label htmlFor="workspace-path">
              <Monitor size={14} /> workspace path on this computer
            </label>
            <div className="form-row">
              <input
                id="workspace-path"
                name="workspace_path"
                value={workspacePath}
                onChange={(e: React.ChangeEvent<HTMLInputElement>) => setWorkspacePath(e.target.value)}
                placeholder="e.g. /Users/you/dev/workspace"
                autoComplete="off"
              />
              <button type="submit" disabled={!workspacePath.trim()} title="browsers can't detect local paths — type the folder you want to share">
                share env with backend
              </button>
            </div>
            {status && <span className="saved-mark">{status}</span>}
          </form>
          {error && <p className="error">{error}</p>}
        </div>
      )}
      {pane === "browser" && env?.reported && (
        <section className="env-card env-card-wide" aria-label="browser client">
          <h3>
            <Globe size={14} /> browser client
          </h3>
          <dl>
            <dt>platform</dt>
            <dd>{env.platform || "—"}</dd>
            <dt>language</dt>
            <dd>{env.language || "—"}</dd>
            <dt>timezone</dt>
            <dd>{env.timezone || "—"}</dd>
            <dt>screen</dt>
            <dd>{env.screen || "—"}</dd>
          </dl>
        </section>
      )}
      {pane === "install" && <InstallCmd />}
    </SplitLayout>
  );
}

/* --- ai providers --- */

type ProviderTab = "zai" | "codex" | "claude" | "local";

const PROVIDER_ITEMS: (SideItem & { id: ProviderTab })[] = [
  { id: "zai", name: "external", desc: "cloud api (openai-compatible)", icon: <Zap size={16} /> },
  { id: "codex", name: "codex cli", desc: "oauth bridge", icon: <Code2 size={16} /> },
  { id: "claude", name: "claude", desc: "claude.ai oauth", icon: <Code2 size={16} /> },
  { id: "local", name: "local mlx", desc: "on-device model", icon: <HardDrive size={16} /> },
];

function ProvidersTab() {
  const [provider, setProvider] = useState<ProviderTab>("zai");

  return (
    <SplitLayout items={PROVIDER_ITEMS} active={provider} on_pick={(id) => setProvider(id as ProviderTab)} label="ai providers">
      {provider === "zai" && <ZaiProvider />}
      {provider === "codex" && <CodexProvider />}
      {provider === "claude" && <ClaudeProvider />}
      {provider === "local" && <LocalProvider />}
    </SplitLayout>
  );
}

type ZaiModal = { mode: "create" } | { mode: "key"; model: string } | null;

function ZaiProvider() {
  const [entries, setEntries] = useState<ZaiModel[]>([]);
  const [active, setActive] = useState("");
  const [keySet, setKeySet] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const [modal, setModal] = useState<ZaiModal>(null);
  const [draftModel, setDraftModel] = useState("");
  const [draftKey, setDraftKey] = useState("");

  const suggestions: string[] = [...new Set([...ZAI_MODELS, ...entries.map((m) => m.model)])];

  useEffect(() => {
    fetch_zai_settings()
      .then((s: ZaiSettings) => {
        setKeySet(s.api_key_set);
        setEntries(s.models);
        setActive(s.model);
      })
      .catch((err: unknown) => setError(err instanceof Error ? err.message : String(err)));
  }, []);

  function apply(s: ZaiSettings) {
    setKeySet(s.api_key_set);
    setEntries(s.models);
    setActive(s.model);
  }

  async function run(req: ZaiModelAction, done: () => void) {
    setStatus("");
    setError("");
    try {
      apply(await zai_model_action(req));
      setStatus("saved");
      done();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  function close() {
    setModal(null);
    setDraftModel("");
    setDraftKey("");
  }

  function createEntry(e: React.FormEvent) {
    e.preventDefault();
    run({ action: "add", model: draftModel.trim(), api_key: draftKey.trim() }, close);
  }

  function saveKey(e: React.FormEvent) {
    e.preventDefault();
    if (modal?.mode !== "key") return;
    run({ action: "set_key", model: modal.model, api_key: draftKey.trim() }, close);
  }

  const creating = modal?.mode === "create";

  return (
    <div className="settings-form">
      <h2 className="sub">
        <KeyRound size={14} /> z.ai · {keySet ? "key saved" : "no key"}
      </h2>
      {entries.length === 0 && <p className="model-empty">no models — create one to use z.ai</p>}
      <ul className="model-list">
        {entries.map((m) => (
          <li key={m.model} className={m.model === active ? "active" : ""}>
            <button
              type="button"
              className="model-pick"
              title="set active"
              onClick={() =>
                m.model !== active && run({ action: "set_active", model: m.model }, () => {})
              }
            >
              <span className="model-name">{m.model}</span>
              <em className={m.api_key_set ? "saved-mark" : "model-empty"}>
                {m.api_key_set ? "token set" : "no token"}
              </em>
            </button>
            <span className="model-actions">
              {m.model === active && <em className="saved-mark">active</em>}
              <button
                type="button"
                className="icon-btn"
                title={`edit token for ${m.model}`}
                onClick={() => {
                  setDraftModel(m.model);
                  setModal({ mode: "key", model: m.model });
                }}
              >
                <Pencil size={14} />
              </button>
              <button
                type="button"
                className="icon-btn"
                title={`remove ${m.model}`}
                onClick={() => run({ action: "remove", model: m.model }, () => {})}
              >
                <Trash2 size={14} />
              </button>
            </span>
          </li>
        ))}
      </ul>
      <div className="form-row">
        <button
          type="button"
          onClick={() => {
            setDraftModel("");
            setModal({ mode: "create" });
          }}
        >
          <Plus size={14} /> create
        </button>
      </div>
      {status && <span className="saved-mark">{status}</span>}
      {error && <p className="error">{error}</p>}
      <Modal
        open={modal !== null}
        title={creating ? "create z.ai model" : "edit token"}
        on_close={close}
      >
        <form className="settings-form" onSubmit={creating ? createEntry : saveKey}>
          <label htmlFor="zai-model-name">model</label>
          <input
            id="zai-model-name"
            value={draftModel}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => setDraftModel(e.target.value)}
            placeholder="glm-4.6"
            list="zai-model-list"
            disabled={!creating}
          />
          <datalist id="zai-model-list">
            {suggestions.map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>
          <label htmlFor="zai-model-key">api key</label>
          <input
            id="zai-model-key"
            type="password"
            value={draftKey}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => setDraftKey(e.target.value)}
            placeholder={creating ? "paste api key for this model" : "•••••••• (type to replace)"}
            autoComplete="off"
          />
          <div className="form-row">
            <button type="submit" disabled={!draftModel.trim() || !draftKey.trim()}>
              {creating ? "create" : "save"}
            </button>
          </div>
          {error && <p className="error">{error}</p>}
        </form>
      </Modal>
    </div>
  );
}

function CodexProvider() {
  return (
    <form className="settings-form">
      <h2 className="sub">codex cli</h2>
      <CodexLogin />
    </form>
  );
}

function ClaudeProvider() {
  return (
    <form className="settings-form">
      <h2 className="sub">claude</h2>
      <ClaudeLogin />
    </form>
  );
}

function LocalProvider() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [selected, setSelected] = useState<string>("");
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    fetch_models()
      .then((list: ModelInfo[]) => {
        setModels(list);
        setSelected(list.find((m) => m.selected)?.name || "");
      })
      .catch((err: unknown) => setError(err instanceof Error ? err.message : String(err)));
  }, []);

  async function pick(e: React.ChangeEvent<HTMLSelectElement>) {
    const name = e.target.value;
    setStatus("");
    setError("");
    try {
      await select_model(name);
      setSelected(name);
      setStatus("selected");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <form className="settings-form">
      <h2 className="sub">
        <Cpu size={14} /> local mlx · {selected ? pretty_name(selected) : "no model selected"} ({models.length} discovered)
      </h2>
      <label htmlFor="local-model">model</label>
      <select id="local-model" value={selected} onChange={pick}>
        <option value="" disabled>
          select a model
        </option>
        {models.map((m) => (
          <option key={m.name} value={m.name} disabled={!m.loadable}>
            {m.selected ? "★ " : ""}
            {pretty_name(m.name)} · {m.engine.toUpperCase()} · {size_label(m.bytes)}
          </option>
        ))}
      </select>
      {status && <span className="saved-mark">{status}</span>}
      {error && <p className="error">{error}</p>}
    </form>
  );
}

/* --- users --- */

const USER_ROLES = ["viewer", "editor", "admin", "super_admin", "owner"] as const;
const MIN_PASSWORD_LEN = 8;

function UsersTab() {
  const [users, set_users] = useState<UserInfo[]>([]);
  const [selected, set_selected] = useState<string | null>(null);
  const [creating, set_creating] = useState(false);
  const [error, set_error] = useState("");

  useEffect(() => {
    reload().catch((err: unknown) => set_error(err instanceof Error ? err.message : String(err)));
  }, []);

  async function reload(): Promise<void> {
    const list = await fetch_users();
    set_users(list);
    set_selected((cur) => (cur && list.some((u) => u.username === cur) ? cur : list[0]?.username ?? null));
  }

  const active = users.find((u) => u.username === selected) ?? null;

  const items: SideItem[] = users.map((u) => ({
    id: u.username,
    name: u.username,
    desc: u.role,
    icon: <User size={16} />,
  }));

  return (
    <>
      <SplitLayout
        items={items}
        active={selected ?? ""}
        on_pick={set_selected}
        label="users"
        footer={
          <button type="button" className="split-item split-new" onClick={() => set_creating(true)}>
            <span className="split-item-icon">
              <Plus size={16} />
            </span>
            <span className="split-item-text">
              <span className="split-item-name">create user</span>
            </span>
          </button>
        }
      >
        {active ? (
          <section className="env-card env-card-wide" aria-label="user detail">
            <h3>
              <User size={14} /> {active.username}
            </h3>
            <dl>
              <dt>username</dt>
              <dd>{active.username}</dd>
              <dt>role</dt>
              <dd>{active.role}</dd>
            </dl>
          </section>
        ) : (
          <p className="empty">no user selected — create one to get started</p>
        )}
        {error && <p className="error">{error}</p>}
      </SplitLayout>
      <CreateUserModal open={creating} on_close={() => set_creating(false)} on_done={reload} />
    </>
  );
}

function CreateUserModal({
  open,
  on_close,
  on_done,
}: {
  open: boolean;
  on_close: () => void;
  on_done: () => Promise<void>;
}) {
  const [username, set_username] = useState("");
  const [password, set_password] = useState("");
  const [role, set_role] = useState<string>("viewer");
  const [status, set_status] = useState("");
  const [error, set_error] = useState("");

  useEffect(() => {
    if (open) {
      set_username("");
      set_password("");
      set_role("viewer");
      set_status("");
      set_error("");
    }
  }, [open]);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    set_status("");
    set_error("");
    try {
      await register_user(username.trim(), password, role);
      set_status(`created ${username.trim()}`);
      await on_done();
      on_close();
    } catch (err: unknown) {
      set_error(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <Modal open={open} title="create user" on_close={on_close}>
      <form className="modal-form" onSubmit={submit}>
        <label htmlFor="new-user-name">username</label>
        <input
          id="new-user-name"
          value={username}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => set_username(e.target.value)}
          autoComplete="off"
        />
        <label htmlFor="new-user-password">password</label>
        <input
          id="new-user-password"
          type="password"
          value={password}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => set_password(e.target.value)}
          placeholder={`min ${MIN_PASSWORD_LEN} chars (user must change on first login)`}
          autoComplete="new-password"
        />
        <label htmlFor="new-user-role">role</label>
        <select
          id="new-user-role"
          value={role}
          onChange={(e: React.ChangeEvent<HTMLSelectElement>) => set_role(e.target.value)}
        >
          {USER_ROLES.map((r) => (
            <option key={r} value={r}>
              {r}
            </option>
          ))}
        </select>
        <button type="submit" disabled={!username.trim() || password.length < MIN_PASSWORD_LEN}>
          create user
        </button>
        {status && <span className="saved-mark">{status}</span>}
        {error && <p className="error">{error}</p>}
      </form>
    </Modal>
  );
}
