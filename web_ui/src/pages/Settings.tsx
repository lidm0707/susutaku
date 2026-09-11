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
  RefreshCw,
  Hand,
  Clock,
  Bell,
  HelpCircle,
} from "lucide-react";
import { Modal } from "../ui/Overlay.jsx";
import { toast } from "../ui/Toast.jsx";
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
  fetch_local_settings,
  save_local_settings,
  fetch_zai_quota,
  zai_say_hi,
  save_zai_schedule,
  type ZaiQuota,
  fetch_codex_usage_latest,
  fetch_codex_usage_history,
  type CodexUsageRow,
  fetch_alert_settings,
  save_alert_settings,
} from "../lib.js";

type Tab = "client" | "providers" | "alerts" | "timezone" | "users";

const ZAI_MODELS = ["glm-4.6", "glm-4.6v", "glm-4.5", "glm-4.5-air", "glm-4.5-flash", "glm-4.5v"] as const;

const TABS: { id: Tab; label: string }[] = [
  { id: "client", label: "client env" },
  { id: "providers", label: "ai providers" },
  { id: "alerts", label: "alerts" },
  { id: "timezone", label: "timezone" },
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
        {tab === "alerts" && <AlertsTab />}
        {tab === "timezone" && <TimezoneTab />}
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
          the client creates one sandbox dir per running agent locally, named
          <code>susutaku-agent-sandbox-&lt;pid&gt;</code>
        </li>
        <li>
          once running, the machine appears in the <strong>machines</strong> dialog
          (sidebar) — its per-agent sandboxes are listed under <strong>agent sandboxes</strong>,
          and stale entries (dead clients) can be removed there anytime
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
          on the server, open the <strong>sandbox</strong> page and press
          <strong> sweep stale</strong> to clean up dead registrations — or run
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
  const [tz, set_tz] = useState<string>(localStorage.getItem(TZ_KEY) || TZ_LOCAL);

  useEffect(() => {
    fetch_zai_settings()
      .then((s: ZaiSettings) => {
        if (s.timezone) {
          set_tz(s.timezone);
          localStorage.setItem(TZ_KEY, s.timezone);
        }
      })
      .catch(() => {});
  }, []);

  return (
    <SplitLayout items={PROVIDER_ITEMS} active={provider} on_pick={(id) => setProvider(id as ProviderTab)} label="ai providers">
      {provider === "codex" && <CodexUsageBlock tz={tz} />}
      {provider === "zai" && <ZaiProvider tz={tz} />}
      {provider === "codex" && <CodexProvider />}
      {provider === "claude" && <ClaudeProvider />}
      {provider === "local" && <LocalProvider />}
      {provider === "local" && <LocalEndpoint />}
    </SplitLayout>
  );
}

type ZaiModal = { mode: "create" } | { mode: "key"; model: string } | null;

const ALERT_ITEMS: (SideItem & { id: "discord" })[] = [
  { id: "discord", name: "discord", desc: "webhook alerts", icon: <Bell size={16} /> },
];

function AlertsTab() {
  const [channel, setChannel] = useState<string>("discord");

  return (
    <SplitLayout items={ALERT_ITEMS} active={channel} on_pick={(id) => setChannel(id)} label="alerts">
      {channel === "discord" && <AlertsProvider />}
    </SplitLayout>
  );
}

/// Own tab: timezone dropdown plus a comparison of what the frontend
/// detects vs what the backend has stored (null = backend uses its own
/// server-local time for the schedule).
function TimezoneTab() {
  const [tz, setTz] = useState<string>(localStorage.getItem(TZ_KEY) || TZ_LOCAL);
  const [backendTz, setBackendTz] = useState<string | null>(null);
  const [hiTime, setHiTime] = useState("");
  const [hiInterval, setHiInterval] = useState<number | null>(null);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const browserTz = Intl.DateTimeFormat().resolvedOptions().timeZone || "unknown";

  useEffect(() => {
    fetch_zai_settings()
      .then((s: ZaiSettings) => {
        setBackendTz(s.timezone);
        setHiTime(s.say_hi_time || "");
        setHiInterval(s.say_hi_interval_mins);
        if (s.timezone) {
          setTz(s.timezone);
          localStorage.setItem(TZ_KEY, s.timezone);
        }
      })
      .catch(() => {});
  }, []);

  async function saveSchedule(nextTz: string) {
    setStatus("");
    setError("");
    try {
      // echo the stored say-hi time back — the backend clears the schedule
      // when say_hi_time arrives as null.
      const s = await save_zai_schedule(hiTime || null, hiInterval, nextTz === TZ_LOCAL ? null : nextTz);
      setBackendTz(s.timezone);
      setTz(s.timezone || TZ_LOCAL);
      localStorage.setItem(TZ_KEY, s.timezone || TZ_LOCAL);
      setStatus("saved");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <div className="timezone-layout">
      <section className="provider-section" aria-label="timezone">
        <h3>
          <Globe size={14} /> timezone
        </h3>
        <div className="form-row tz-field" title="what is timezone? — the IANA zone used to fire the daily say hi and to show quota reset times">
          <select
            aria-label="timezone"
            value={tz}
            onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
              setTz(e.target.value);
              void saveSchedule(e.target.value);
            }}
          >
            <option value={TZ_LOCAL}>local time</option>
            {timezone_options().map((z) => (
              <option key={z} value={z}>{z}</option>
            ))}
          </select>
          <HelpCircle size={13} />
        </div>
        {status && <span className="saved-mark">{status}</span>}
        {error && <span className="error">{error}</span>}
      </section>
      <section className="provider-section" aria-label="timezone sources">
        <h3>detected vs stored</h3>
        <dl className="tz-source-list">
          <div>
            <dt>frontend detects</dt>
            <dd>{browserTz}</dd>
          </div>
          <div>
            <dt>backend receives</dt>
            <dd>{backendTz ?? "not set — backend uses server-local time"}</dd>
          </div>
        </dl>
      </section>
    </div>
  );
}

function AlertsProvider() {
  const [url, setUrl] = useState("");
  const [webhookSet, setWebhookSet] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    fetch_alert_settings()
      .then((s) => setWebhookSet(s.webhook_set))
      .catch(() => setWebhookSet(false));
  }, []);

  async function save(webhook_url: string | null) {
    setStatus("");
    setError("");
    try {
      const res = await save_alert_settings(webhook_url);
      setWebhookSet(res.webhook_set);
      setUrl("");
      setStatus(webhook_url ? "saved" : "cleared");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  function submit(e: React.FormEvent) {
    e.preventDefault();
    void save(url.trim() ? url.trim() : null);
  }

  return (
    <section className="provider-section" aria-label="alerts">
      <h3>discord webhook alerts</h3>
      <p className="sub">
        {webhookSet ? "webhook set" : "no webhook — agent finish events stay in-app"}
      </p>
      <form onSubmit={submit}>
        <div className="form-row">
          <input
            type="password"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder={webhookSet ? "replace webhook url…" : "discord webhook url…"}
            autoComplete="off"
          />
        </div>
        <div className="form-row">
          <button type="submit">{webhookSet ? "update" : "save"}</button>
          {webhookSet && (
            <button type="button" onClick={() => void save(null)}>
              clear
            </button>
          )}
          {status && <span className="saved-mark">{status}</span>}
          {error && <span className="error">{error}</span>}
        </div>
      </form>
    </section>
  );
}

function ZaiProvider({ tz }: { tz: string }) {
  const [entries, setEntries] = useState<ZaiModel[]>([]);
  const [active, setActive] = useState("");
  const [keySet, setKeySet] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const [modal, setModal] = useState<ZaiModal>(null);
  const [draftModel, setDraftModel] = useState("");
  const [draftKey, setDraftKey] = useState("");
  const [hiTime, setHiTime] = useState("");
  const [hiInterval, setHiInterval] = useState("");
  const [schedStatus, setSchedStatus] = useState("");
  const [schedError, setSchedError] = useState("");

  const suggestions: string[] = [...new Set([...ZAI_MODELS, ...entries.map((m) => m.model)])];

  useEffect(() => {
    fetch_zai_settings()
      .then((s: ZaiSettings) => {
        setKeySet(s.api_key_set);
        setEntries(s.models);
        setActive(s.model);
        setHiTime(s.say_hi_time || "");
        setHiInterval(s.say_hi_interval_mins != null ? String(s.say_hi_interval_mins) : "");
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

  async function saveSchedule(nextHiTime: string, nextInterval: string = hiInterval) {
    setSchedStatus("");
    setSchedError("");
    const mins = Number(nextInterval);
    const interval = Number.isFinite(mins) && mins >= 1 ? Math.floor(mins) : null;
    try {
      const s = await save_zai_schedule(nextHiTime || null, interval, tz === TZ_LOCAL ? null : tz);
      setHiTime(s.say_hi_time || "");
      setHiInterval(s.say_hi_interval_mins != null ? String(s.say_hi_interval_mins) : "");
      setSchedStatus("saved");
    } catch (err: unknown) {
      setSchedError(err instanceof Error ? err.message : String(err));
    }
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
      <ZaiQuotaBlock tz={tz} />
      <section className="provider-section" aria-label="zai schedule">
        <h3>daily say hi</h3>
        <p className="sub">
          {hiTime
            ? `z.ai say hi at ${hiTime}${tz === TZ_LOCAL ? " (local time)" : ` (${tz})`}${hiInterval ? `, every ${hiInterval} min` : ""}`
            : "no daily say hi scheduled"}
        </p>
        <form
          onSubmit={(e: React.FormEvent) => {
            e.preventDefault();
            void saveSchedule(hiTime);
          }}
        >
          <div className="form-row">
            <input
              type="time"
              aria-label="say hi start time"
              value={hiTime}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => setHiTime(e.target.value)}
            />
            <input
              type="number"
              aria-label="say hi repeat every (minutes)"
              min={1}
              step={1}
              className="say-hi-interval"
              title="repeat every N minutes after the start time (empty = once a day)"
              placeholder="every …"
              value={hiInterval}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => setHiInterval(e.target.value)}
            />
            <button type="submit" disabled={!hiTime}>save</button>
            {hiTime && (
              <button
                type="button"
                onClick={() => {
                  setHiTime("");
                  setHiInterval("");
                  void saveSchedule("", "");
                }}
              >
                clear
              </button>
            )}
          </div>
          {schedStatus && <span className="saved-mark">{schedStatus}</span>}
          {schedError && <span className="error">{schedError}</span>}
        </form>
      </section>
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

type QuotaState = { quota: ZaiQuota } | { error: true } | null;

const QUOTA_MINT_MAX = 50;
const QUOTA_LEMON_MAX = 80;
const REPLY_PREVIEW_LEN = 120;
const TZ_LOCAL = "local";
const TZ_KEY = "susutaku:tz";

function timezone_options(): string[] {
  const supported = (Intl as unknown as { supportedValuesOf?: (k: string) => string[] })
    .supportedValuesOf?.("timeZone");
  return supported ? supported.filter((z) => z.includes("/")) : [TZ_LOCAL];
}

function quota_mood(pct: number): string {
  if (pct < QUOTA_MINT_MAX) return "genki";
  if (pct < QUOTA_LEMON_MAX) return "mma...";
  return "abunai!";
}

function reset_time(ms: number, tz: string): string {
  const fmt = new Intl.DateTimeFormat("en-GB", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
    ...(tz !== TZ_LOCAL ? { timeZone: tz } : {}),
  });
  return fmt.format(new Date(ms));
}

/// Short UTC offset of the display timezone, e.g. "UTC+7".
function tz_label(tz: string): string {
  const zone = tz === TZ_LOCAL ? Intl.DateTimeFormat().resolvedOptions().timeZone : tz;
  try {
    const parts = new Intl.DateTimeFormat("en-GB", { timeZone: zone, timeZoneName: "shortOffset" }).formatToParts();
    return parts.find((p) => p.type === "timeZoneName")?.value || "";
  } catch {
    return "";
  }
}

function ZaiQuotaBlock({ tz }: { tz: string }) {
  const [state, setState] = useState<QuotaState>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    fetch_zai_quota()
      .then((quota) => setState({ quota }))
      .catch(() => setState({ error: true }));
  }, []);

  async function refresh() {
    setBusy(true);
    try {
      setState({ quota: await fetch_zai_quota() });
    } catch (err: unknown) {
      toast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      setBusy(false);
    }
  }

  async function say_hi() {
    setBusy(true);
    try {
      const res = await zai_say_hi();
      toast(res.reply.slice(0, REPLY_PREVIEW_LEN), "success");
      setState({ quota: await fetch_zai_quota() });
    } catch (err: unknown) {
      toast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      setBusy(false);
    }
  }

  if (state === null) return null;
  if ("error" in state) return <p className="model-empty zai-quota-note">quota unavailable</p>;

  const { tokens_used_pct, time_limit_reset_ms } = state.quota;
  const mood = quota_bar_class(tokens_used_pct);

  return (
    <section className="zai-quota" aria-label="z.ai quota">
      <div className="zai-quota-bar" role="progressbar" aria-valuenow={tokens_used_pct} aria-valuemin={0} aria-valuemax={100}>
        <span className={`zai-quota-fill ${mood}`} style={{ width: `${Math.min(100, Math.max(0, tokens_used_pct))}%` }} />
      </div>
      <p className="zai-quota-label">
        <span>{tokens_used_pct}% used · {quota_mood(tokens_used_pct)}</span>
        {time_limit_reset_ms !== null && (
          <span>
            <Clock size={12} /> window resets {reset_time(time_limit_reset_ms, tz)} ({tz_label(tz)})
          </span>
        )}
        <span className="zai-quota-actions">
          <button type="button" className="icon-btn" title="refresh quota" disabled={busy} onClick={refresh}>
            <RefreshCw size={14} />
          </button>
          <button type="button" className="icon-btn" title="say hi" disabled={busy} onClick={say_hi}>
            <Hand size={14} /> say hi
          </button>
        </span>
      </p>
    </section>
  );
}

type CodexUsageState =
  | { latest: CodexUsageRow | null; history: CodexUsageRow[] }
  | { error: true }
  | null;

const CODEX_USAGE_HISTORY_POINTS = 12;

/// Full date + time for multi-day reset windows (weekly etc.).
function reset_datetime(ms: number, tz: string): string {
  const fmt = new Intl.DateTimeFormat("en-GB", {
    day: "2-digit",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
    ...(tz !== TZ_LOCAL ? { timeZone: tz } : {}),
  });
  return fmt.format(new Date(ms));
}

function CodexUsageBlock({ tz }: { tz: string }) {
  const [state, setState] = useState<CodexUsageState>(null);
  const [busy, setBusy] = useState(false);

  async function load() {
    setBusy(true);
    try {
      const [latest, history] = await Promise.all([
        fetch_codex_usage_latest(),
        fetch_codex_usage_history(CODEX_USAGE_HISTORY_POINTS),
      ]);
      setState({ latest, history });
    } catch {
      setState({ error: true });
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    load();
  }, []);

  return (
    <section className="quota-board" aria-label="codex usage">
      <p className="quota-board-head">
        <span className="quota-board-title">codex usage (sampled)</span>
        <button type="button" className="icon-btn" title="refresh codex usage" disabled={busy} onClick={load}>
          <RefreshCw size={14} />
        </button>
      </p>
      {state === null && <p className="model-empty">loading…</p>}
      {state !== null && "error" in state && <p className="model-empty">codex usage unavailable</p>}
      {state !== null && !("error" in state) && (state.latest === null ? (
        <p className="model-empty">no samples yet — scheduler stores one every few minutes</p>
      ) : (
        <CodexUsageLatest row={state.latest} tz={tz} history={state.history} />
      ))}
    </section>
  );
}

function CodexUsageLatest({
  row,
  tz,
  history,
}: {
  row: CodexUsageRow;
  tz: string;
  history: CodexUsageRow[];
}) {
  return (
    <div className="codex-usage">
      <p className="model-empty zai-quota-note">
        plan {row.plan_type ?? "?"} · sampled {reset_datetime(Date.parse(row.captured_at), tz)} ({tz_label(tz)})
      </p>
      <div className="codex-usage-windows">
        <CodexUsageWindow
          label="5h window"
          pct={row.primary_used_percent}
          reset={row.primary_resets_at}
          tz={tz}
        />
        {row.secondary_used_percent !== null && (
          <CodexUsageWindow
            label="weekly"
            pct={row.secondary_used_percent}
            reset={row.secondary_resets_at}
            tz={tz}
          />
        )}
      </div>
      <CodexUsageHistory history={history} tz={tz} />
    </div>
  );
}

function CodexUsageWindow({
  label,
  pct,
  reset,
  tz,
}: {
  label: string;
  pct: number | null;
  reset: string | null;
  tz: string;
}) {
  const shown = pct ?? 0;
  const countdown = reset !== null ? until_label(Date.parse(reset)) : null;
  return (
    <div className="codex-usage-window">
      <p className="codex-usage-window-head">
        <span className="codex-usage-window-label">{label}</span>
        <span className="codex-usage-window-pct">{pct !== null ? `${pct}%` : "—"}</span>
      </p>
      <span
        className="zai-quota-bar codex-usage-bar"
        role="progressbar"
        aria-valuenow={shown}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={`${label} usage`}
      >
        <span className={`zai-quota-fill ${quota_bar_class(shown)}`} style={{ width: `${Math.min(100, Math.max(0, shown))}%` }} />
      </span>
      <p className="codex-usage-window-reset">
        {countdown !== null ? (
          <>
            <Clock size={11} /> resets in {countdown}
          </>
        ) : (
          "\u00a0"
        )}
      </p>
    </div>
  );
}

/// Humanized distance, e.g. "3h 20m" / "42m" / "5d".
function until_label(ms: number): string {
  const mins = Math.max(0, Math.round((ms - Date.now()) / 60_000));
  if (mins >= 60 * 24) return `${Math.floor(mins / (60 * 24))}d ${Math.floor((mins % (60 * 24)) / 60)}h`;
  if (mins >= 60) return `${Math.floor(mins / 60)}h ${mins % 60}m`;
  return `${mins}m`;
}

function CodexUsageHistory({ history, tz }: { history: CodexUsageRow[]; tz: string }) {
  if (history.length < 2) return null;
  const points = history.slice(0, CODEX_USAGE_HISTORY_POINTS).reverse();
  return (
    <div className="codex-usage-history">
      <div className="codex-usage-spark" role="img" aria-label="5h usage over recent samples">
        {points.map((row) => {
          const pct = row.primary_used_percent ?? 0;
          return (
            <span
              key={row.captured_at}
              className={`codex-usage-spark-bar ${quota_bar_class(pct)}`}
              title={`${reset_datetime(Date.parse(row.captured_at), tz)} · ${row.primary_used_percent ?? "—"}%`}
              style={{ height: `${Math.max(4, Math.min(100, pct))}%` }}
            />
          );
        })}
      </div>
      <p className="codex-usage-history-note">
        last {points.length} samples · oldest → newest
      </p>
    </div>
  );
}

function quota_bar_class(pct: number): string {
  return pct < QUOTA_MINT_MAX ? "mint" : pct < QUOTA_LEMON_MAX ? "lemon" : "pink";
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

function LocalEndpoint() {
  const [endpoint, setEndpoint] = useState("");

  useEffect(() => {
    fetch_local_settings()
      .then((s) => setEndpoint(s.endpoint))
      .catch(() => {});
  }, []);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    try {
      await save_local_settings(endpoint.trim());
      toast("local endpoint saved", "success");
    } catch (err: unknown) {
      toast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  return (
    <form className="settings-form" onSubmit={submit}>
      <h2 className="sub">local model</h2>
      <label htmlFor="local-endpoint">endpoint</label>
      <input
        id="local-endpoint"
        value={endpoint}
        onChange={(e: React.ChangeEvent<HTMLInputElement>) => setEndpoint(e.target.value)}
        placeholder="http://127.0.0.1:8992"
        autoComplete="off"
      />
      <p className="model-empty">openai-compatible endpoint (/v1/chat/completions)</p>
      <div className="form-row">
        <button type="submit">save</button>
      </div>
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
