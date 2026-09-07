import { useState } from "react";
import { Link } from "react-router-dom";
import { ArrowLeft, FileText, Play, Plus, Trash2 } from "lucide-react";
import {
  chat_zai,
  render_prompt,
  type ChatReply,
  type PromptSection,
  type RenderedPrompt,
} from "../lib.js";

const ROLES = ["system", "instructions", "context", "tools"];

export default function Prompts() {
  const [sections, setSections] = useState<PromptSection[]>([
    { role: "system", body: "" },
  ]);
  const [preview, setPreview] = useState<RenderedPrompt | null>(null);
  const [message, setMessage] = useState("");
  const [reply, setReply] = useState<ChatReply | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  function update(i: number, patch: Partial<PromptSection>) {
    setSections(sections.map((s, j) => (j === i ? { ...s, ...patch } : s)));
  }

  function add() {
    setSections([...sections, { role: "context", body: "" }]);
  }

  function remove(i: number) {
    setSections(sections.filter((_, j) => j !== i));
  }

  async function previewPrompt() {
    setError("");
    setPreview(null);
    try {
      setPreview(await render_prompt(sections.filter((s) => s.body.trim())));
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function test() {
    setError("");
    setReply(null);
    setBusy(true);
    try {
      const system = sections
        .filter((s) => s.body.trim())
        .map((s) => ({ role: s.role, body: s.body }));
      setReply(await chat_zai(message, "", system));
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="chat">
      <header>
        <h1><Link to="/prompts">prompts</Link></h1>
        <span className="sub">
          {preview
            ? `${preview.chars} / ${preview.max_chars} chars`
            : "compose a system prompt"}
        </span>
        <nav className="nav">
          <button className="codex-login" onClick={previewPrompt}>
            <FileText size={14} /> render
          </button>
          <Link to="/chat" title="back to chat"><ArrowLeft size={16} /></Link>
        </nav>
      </header>
      {error && <p className="error">{error}</p>}
      <section className="log">
        {sections.map((s, i) => (
          <div key={i} className="model-row" style={{ alignItems: "flex-start" }}>
            <select
              value={s.role}
              onChange={(e) => update(i, { role: e.target.value })}
              className="codex-login"
              style={{ width: "9rem" }}
            >
              {ROLES.map((r) => (
                <option key={r} value={r}>{r}</option>
              ))}
            </select>
            <textarea
              value={s.body}
              onChange={(e) => update(i, { body: e.target.value })}
              placeholder={`${s.role} body…`}
              rows={3}
              style={{ flex: 1 }}
            />
            <button className="codex-login" onClick={() => remove(i)} title="remove section">
              <Trash2 size={14} />
            </button>
          </div>
        ))}
        <button className="model-row" onClick={add}>
          <Plus size={14} />
          <span className="model-name">add section</span>
        </button>
        {preview && (
          <pre className="log" style={{ whiteSpace: "pre-wrap" }}>{preview.rendered}</pre>
        )}
      </section>
      <section className="log">
        <div className="model-row">
          <input
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            placeholder="test message (z.ai)…"
            style={{ flex: 1 }}
          />
          <button className="codex-login" onClick={test} disabled={busy || !message.trim()}>
            <Play size={14} /> {busy ? "…" : "test"}
          </button>
        </div>
        {reply && <pre className="log" style={{ whiteSpace: "pre-wrap" }}>{reply.reply}</pre>}
      </section>
    </main>
  );
}
