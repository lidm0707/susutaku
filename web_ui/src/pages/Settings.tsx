import { useEffect, useState } from "react";
import { KeyRound, Monitor, Cpu } from "lucide-react";
import {
  fetch_zai_settings,
  save_zai_settings,
  fetch_client_env,
  save_client_env,
  codex_status,
  fetch_models,
  type ZaiSettings,
  type ClientEnv,
  type CodexStatus,
  type ModelInfo,
} from "../lib.js";

type Tab = "client" | "providers";

const TABS: { id: Tab; label: string }[] = [
  { id: "client", label: "client env" },
  { id: "providers", label: "ai providers" },
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
      </section>
    </main>
  );
}

function ClientEnvTab() {
  const [env, setEnv] = useState<ClientEnv | null>(null);
  const [workspacePath, setWorkspacePath] = useState("");
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");

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

  return (
    <>
      {env?.reported ? (
        <dl className="env-table">
          <dt>platform</dt>
          <dd>{env.platform || "—"}</dd>
          <dt>os / browser</dt>
          <dd>{env.user_agent || "—"}</dd>
          <dt>language</dt>
          <dd>{env.language || "—"}</dd>
          <dt>timezone</dt>
          <dd>{env.timezone || "—"}</dd>
          <dt>screen</dt>
          <dd>{env.screen || "—"}</dd>
          <dt>workspace path</dt>
          <dd>{env.workspace_path || "—"}</dd>
        </dl>
      ) : (
        <p className="sub" style={{ textAlign: "center" }}>
          this computer's environment has not been shared with the backend yet
        </p>
      )}
      <form className="settings-form" onSubmit={share}>
        <label>
          <Monitor size={14} /> workspace path on this computer
        </label>
        <input
          value={workspacePath}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => setWorkspacePath(e.target.value)}
          placeholder="/Users/you/dev/workspace"
          autoComplete="off"
        />
        <button type="submit">share env with backend</button>
        {status && <span className="saved-mark">{status}</span>}
      </form>
      {error && <p className="error">{error}</p>}
    </>
  );
}

function ProvidersTab() {
  return (
    <>
      <ZaiProvider />
      <CodexProvider />
      <LocalProvider />
    </>
  );
}

function ZaiProvider() {
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [keySet, setKeySet] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    fetch_zai_settings()
      .then((s: ZaiSettings) => {
        setKeySet(s.api_key_set);
        setModel(s.model || "");
      })
      .catch((err: unknown) => setError(err instanceof Error ? err.message : String(err)));
  }, []);

  async function save(e: React.FormEvent) {
    e.preventDefault();
    setStatus("");
    setError("");
    try {
      const s: ZaiSettings = await save_zai_settings(apiKey, model);
      setKeySet(s.api_key_set);
      setModel(s.model || "");
      setApiKey("");
      setStatus("saved");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <form className="settings-form" onSubmit={save}>
      <h2 className="sub">
        <KeyRound size={14} /> z.ai · {keySet ? "key saved" : "no key"}
      </h2>
      <label>api key {keySet && <em className="saved-mark">(saved)</em>}</label>
      <input
        type="password"
        value={apiKey}
        onChange={(e: React.ChangeEvent<HTMLInputElement>) => setApiKey(e.target.value)}
        placeholder={keySet ? "•••••••• (type to replace)" : "paste your z.ai api key"}
        autoComplete="off"
      />
      <label>model</label>
      <input
        value={model}
        onChange={(e: React.ChangeEvent<HTMLInputElement>) => setModel(e.target.value)}
        placeholder="glm-4.6"
      />
      <button type="submit">save</button>
      {status && <span className="saved-mark">{status}</span>}
      {error && <p className="error">{error}</p>}
    </form>
  );
}

function CodexProvider() {
  const [status, setStatus] = useState<CodexStatus | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    codex_status()
      .then(setStatus)
      .catch((err: unknown) => setError(err instanceof Error ? err.message : String(err)));
  }, []);

  return (
    <form className="settings-form">
      <h2 className="sub">codex cli · {status?.status ?? "checking…"}</h2>
      {error && <p className="error">{error}</p>}
    </form>
  );
}

function LocalProvider() {
  const [selected, setSelected] = useState<string>("");
  const [count, setCount] = useState(0);
  const [error, setError] = useState("");

  useEffect(() => {
    fetch_models()
      .then((models: ModelInfo[]) => {
        setCount(models.length);
        setSelected(models.find((m) => m.selected)?.name || "");
      })
      .catch((err: unknown) => setError(err instanceof Error ? err.message : String(err)));
  }, []);

  return (
    <form className="settings-form">
      <h2 className="sub">
        <Cpu size={14} /> local mlx · {selected || "no model selected"} ({count} discovered)
      </h2>
      {error && <p className="error">{error}</p>}
    </form>
  );
}
