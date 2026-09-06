import { useEffect, useState } from "react";

const API_BASE = import.meta.env.VITE_API_BASE || "";
const MAX_TOKENS = 512;

const THINK_OPEN = "<think>";
const THINK_CLOSE = "</think>";

function split_thinking(text) {
  const open = text.indexOf(THINK_OPEN);
  const close = text.indexOf(THINK_CLOSE);
  if (open === -1) return { thinking: "", reply: text.trim() };
  const thinking = (
    open + THINK_OPEN.length < close ? text.slice(open + THINK_OPEN.length, close) : text.slice(0, open)
  ).trim();
  const reply = (text.slice(0, open) + text.slice(close + THINK_CLOSE.length)).trim();
  return { thinking, reply };
}

export default function App() {
  const [messages, setMessages] = useState([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [search, setSearch] = useState("auto");
  const [tok, setTok] = useState("normal");
  const [models, setModels] = useState([]);
  const [model, setModel] = useState("");
  const [error, setError] = useState("");
  const engine = models.find((m) => m.name === model)?.engine || "mlx";

  async function pick(name) {
    const res = await fetch(`${API_BASE}/api/models/select`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name }),
    });
    if (!res.ok) throw new Error(await res.text());
    setModel(name);
  }

  useEffect(() => {
    fetch(`${API_BASE}/api/models`)
      .then((r) => r.json())
      .then(async (list) => {
        setModels(list);
        const sel = list.find((m) => m.selected) || list[0];
        if (sel) {
          setModel(sel.name);
          if (!sel.selected && sel.loadable) await pick(sel.name).catch(() => {});
        }
      })
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function send(e) {
    e.preventDefault();
    const text = input.trim();
    if (!text || busy) return;
    setInput("");
    setError("");
    setBusy(true);
    setMessages((m) => [...m, { role: "user", text }, { role: "assistant", text: "", pending: true }]);
    try {
      const res = await fetch(`${API_BASE}/api/chat`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ message: text, max_tokens: MAX_TOKENS, search, tokenizer: tok }),
      });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      const data = await res.json();
      const { thinking, reply } = split_thinking(data.reply);
      setMessages((m) =>
        m.map((msg, i) =>
          i === m.length - 1
            ? { role: "assistant", text: reply, thinking, model: data.model, prompt_tps: data.prompt_tps, tps: data.decode_tps, searched: data.searched, tok: data.tokenizer }
            : msg
        )
      );
    } catch (err) {
      setError(String(err.message || err));
      setMessages((m) => m.filter((msg, i) => !(i === m.length - 1 && msg.pending)));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="chat">
      <header>
        <h1>susutaku</h1>
        <span className="sub">{model || "—"} · {engine === "gguf" ? "🧊 gguf" : "⚡ mlx"} · inline</span>
      </header>
      <section className="log">
        {messages.length === 0 && <p className="empty">Say something to the model.</p>}
        {messages.map((m, i) => (
          <div key={i} className={`bubble ${m.role}`}>
            {m.thinking && (
              <details className="thinking">
                <summary>💭 thinking</summary>
                <pre>{m.thinking}</pre>
              </details>
            )}
            <p>{m.text || (m.pending ? "…" : "")}</p>
            {m.tps && <small>{m.model} · prompt {m.prompt_tps.toFixed(1)} tok/s · decode {m.tps.toFixed(1)} tok/s{m.searched ? " · 🌐 searched" : ""}{m.tok === "katgpt" ? " · 🧩 katgpt" : ""}</small>}
          </div>
        ))}
      </section>
      {error && <p className="error">{error}</p>}
      <form onSubmit={send}>
        <select
          className="search-toggle"
          value={model}
          onChange={(e) => pick(e.target.value).catch((err) => setError(String(err.message || err)))}
          title="model"
        >
          {models.map((m) => (
            <option key={m.name} value={m.name} disabled={!m.loadable}>
              {m.selected ? "★ " : ""}
              {m.engine === "gguf" ? "🧊" : "⚡"} {m.name}
            </option>
          ))}
        </select>
        <select
          className="search-toggle"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          title="web search mode"
        >
          <option value="auto">🌐 auto</option>
          <option value="on">🌐 always</option>
          <option value="off">· off</option>
        </select>
        <select
          className="search-toggle"
          value={tok}
          onChange={(e) => setTok(e.target.value)}
          title="tokenizer"
        >
          <option value="normal">⚙ normal</option>
          <option value="katgpt">🧩 katgpt</option>
        </select>
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder={busy ? "generating…" : "type a message"}
          disabled={busy}
          autoFocus
        />
        <button type="submit" disabled={busy || !input.trim()}>
          Send
        </button>
      </form>
    </main>
  );
}
