import { useEffect, useState } from "react";
import {
  Brain,
  Send,
  Snowflake,
  Zap,
} from "lucide-react";
import { API_BASE, chat_codex, chat_zai, fetch_codex_models, fetch_models, pretty_name, select_model, size_label, type ChatReply, type CodexModel, type ModelInfo } from "../lib.js";
import CodexLogin from "../components/CodexLogin.tsx";

interface Msg {
  role: "user" | "assistant";
  text: string;
  pending?: boolean;
  thinking?: string;
  model?: string;
  prompt_tps?: number;
  tps?: number;
  searched?: boolean;
  tok?: string;
}

const CODEX = "codex";
const ZAI = "zai";
const DEFAULT_CODEX_MODEL = "gpt-5-codex"; // used until the account's model list loads

const MAX_TOKENS = 512;

const THINK_OPEN = "<think>";
const THINK_CLOSE = "</think>";

function split_thinking(text: string): { thinking: string; reply: string } {
  const open = text.indexOf(THINK_OPEN);
  const close = text.indexOf(THINK_CLOSE);
  if (open === -1) return { thinking: "", reply: text.trim() };
  const thinking = (
    open + THINK_OPEN.length < close ? text.slice(open + THINK_OPEN.length, close) : text.slice(0, open)
  ).trim();
  const reply = (text.slice(0, open) + text.slice(close + THINK_CLOSE.length)).trim();
  return { thinking, reply };
}

export function EngineIcon({ engine }: { engine: string }) {
  return engine === "gguf" ? <Snowflake size={14} /> : <Zap size={14} />;
}

export default function Chat() {
  const [messages, setMessages] = useState<Msg[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [search, setSearch] = useState("auto");
  const [tok, setTok] = useState("normal");
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [model, setModel] = useState("");
  const [codexModels, setCodexModels] = useState<CodexModel[]>([]);
  const [codexModel, setCodexModel] = useState(DEFAULT_CODEX_MODEL);
  const [zaiModel, setZaiModel] = useState("");
  const [error, setError] = useState("");
  const selected = models.find((m) => m.name === model);

  useEffect(() => {
    fetch_models()
      .then(async (list) => {
        setModels(list);
        const sel = list.find((m) => m.selected) || list[0];
        if (sel) {
          setModel(sel.name);
          if (!sel.selected && sel.loadable) await pick(sel.name).catch(() => {});
        }
      })
      .catch(() => {});
    fetch_codex_models()
      .then((list) => {
        setCodexModels(list);
        if (list.length && !list.some((m) => m.id === codexModel)) {
          setCodexModel(list[0].id);
        }
      })
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function pick(next: string) {
    if (next === CODEX || next === ZAI) {
      setModel(next);
      return;
    }
    await select_model(next);
    setModel(next);
  }

  async function send(e: React.FormEvent) {
    e.preventDefault();
    const text = input.trim();
    if (!text || busy) return;
    setInput("");
    setError("");
    setBusy(true);
    setMessages((m) => [...m, { role: "user", text }, { role: "assistant", text: "", pending: true }]);
    try {
      const data: ChatReply =
        model === CODEX
          ? await chat_codex(text, codexModel)
          : model === ZAI
            ? await chat_zai(text, zaiModel)
            : await (
              await fetch(`${API_BASE}/api/chat`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ message: text, max_tokens: MAX_TOKENS, search, tokenizer: tok }),
              })
            ).json();
      if (data.reply === undefined) throw new Error(`${data.status || ""} ${JSON.stringify(data)}`);
      const { thinking, reply } = split_thinking(data.reply);
      setMessages((m) =>
        m.map((msg, i) =>
          i === m.length - 1
            ? { role: "assistant", text: reply, thinking, model: data.model, prompt_tps: data.prompt_tps, tps: data.decode_tps, searched: data.searched, tok: data.tokenizer }
            : msg
        )
      );
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
      setMessages((m) => m.filter((msg, i) => !(i === m.length - 1 && msg.pending)));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="chat">
      <header>
        <h1>susutaku</h1>
        <span className="sub">
          {model === CODEX
            ? `codex · ${codexModel}`
            : model === ZAI
              ? `z.ai · ${zaiModel || "glm-4.6"}`
              : selected
              ? `${pretty_name(selected.name)} · ${selected.engine.toUpperCase()} · ${size_label(selected.bytes)} · inline`
              : "—"}
        </span>
        <nav className="nav">
          <CodexLogin />
        </nav>
      </header>
      <section className="log">
        {messages.length === 0 && <p className="empty">Say something to the model.</p>}
        {messages.map((m, i) => (
          <div key={i} className={`bubble ${m.role}`}>
            {m.thinking && (
              <details className="thinking">
                <summary><Brain size={12} /> thinking</summary>
                <pre>{m.thinking}</pre>
              </details>
            )}
            <p>{m.text || (m.pending ? "…" : "")}</p>
            {m.tps && <small>{m.model} · prompt {m.prompt_tps?.toFixed(1)} tok/s · decode {m.tps.toFixed(1)} tok/s{m.searched ? " · searched" : ""}{m.tok === "katgpt" ? " · katgpt" : ""}</small>}
          </div>
        ))}
      </section>
      {error && <p className="error">{error}</p>}
      <form onSubmit={send}>
        <select
          className="search-toggle"
          value={model}
          onChange={(e) => pick(e.target.value).catch((err: unknown) => setError(err instanceof Error ? err.message : String(err)))}
          title="model"
        >
          <option value={CODEX}>Codex (ChatGPT)</option>
          <option value={ZAI}>Z.ai (GLM)</option>
          {models.map((m) => (
            <option key={m.name} value={m.name} disabled={!m.loadable}>
              {m.selected ? "★ " : ""}{pretty_name(m.name)} ({m.engine.toUpperCase()})
            </option>
          ))}
        </select>
        {model === CODEX && (
          <select
            className="search-toggle"
            value={codexModel}
            onChange={(e: React.ChangeEvent<HTMLSelectElement>) => setCodexModel(e.target.value)}
            title="codex model"
          >
            {codexModels.map((m) => (
              <option key={m.id} value={m.id}>
                {m.label}
              </option>
            ))}
          </select>
        )}
        {model === ZAI && (
          <input
            className="search-toggle zai-model"
            value={zaiModel}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => setZaiModel(e.target.value)}
            placeholder="glm-4.6"
            title="z.ai model"
          />
        )}
        <select
          className="search-toggle"
          value={search}
          onChange={(e: React.ChangeEvent<HTMLSelectElement>) => setSearch(e.target.value)}
          title="web search mode"
        >
          <option value="auto">auto</option>
          <option value="on">always</option>
          <option value="off">off</option>
        </select>
        <select
          className="search-toggle"
          value={tok}
          onChange={(e: React.ChangeEvent<HTMLSelectElement>) => setTok(e.target.value)}
          title="tokenizer"
        >
          <option value="normal">normal</option>
          <option value="katgpt">katgpt</option>
        </select>
        <input
          value={input}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => setInput(e.target.value)}
          placeholder={busy ? "generating…" : "type a message"}
          disabled={busy}
          autoFocus
        />
        <button type="submit" disabled={busy || !input.trim()} title="send">
          <Send size={16} />
        </button>
      </form>
    </main>
  );
}
