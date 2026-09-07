import { useState } from "react";
import { FileText, Play, Plus, Trash2 } from "lucide-react";
import {
  chat_zai,
  render_prompt,
  type ChatReply,
  type PromptSection,
  type RenderedPrompt,
} from "../lib.js";
import { Button, IconButton, Select, TextArea } from "../ui/controls.js";

const ROLES = ["system", "instructions", "context", "tools"] as const;

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
    <main className="chat kanban-page agents-page">
      <header>
        <h1>prompts</h1>
        <span className="sub">
          {preview
            ? `${preview.chars} / ${preview.max_chars} chars`
            : "compose a system prompt"}
        </span>
      </header>
      {error && <p className="error">{error}</p>}
      <div className="prompts-layout">
        <section className="agent-editor">
          {sections.map((s, i) => (
            <div key={i} className="prompt-section">
              <div className="prompt-section-head">
                <Select
                  aria-label="section role"
                  value={s.role}
                  onChange={(e) => update(i, { role: e.target.value })}
                >
                  {ROLES.map((r) => (
                    <option key={r} value={r}>{r}</option>
                  ))}
                </Select>
                <IconButton title="remove section" onClick={() => remove(i)}>
                  <Trash2 size={14} />
                </IconButton>
              </div>
              <TextArea
                value={s.body}
                onChange={(e) => update(i, { body: e.target.value })}
                placeholder={`${s.role} body…`}
                rows={3}
                spellCheck={false}
              />
            </div>
          ))}
          <Button variant="ghost" className="agents-new" onClick={add}>
            <Plus size={14} /> add section
          </Button>
          {preview && (
            <pre className="prompt-preview">{preview.rendered}</pre>
          )}
          <footer className="agent-editor-foot">
            <Button variant="primary" type="button" onClick={previewPrompt}>
              <FileText size={14} /> render
            </Button>
          </footer>
        </section>
        <section className="agent-editor">
          <TextArea
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            placeholder="test message (z.ai)…"
            rows={4}
          />
          {reply && <pre className="prompt-preview">{reply.reply}</pre>}
          <footer className="agent-editor-foot">
            <Button
              variant="primary"
              type="button"
              onClick={test}
              disabled={busy || !message.trim()}
            >
              <Play size={14} /> {busy ? "…" : "test"}
            </Button>
          </footer>
        </section>
      </div>
    </main>
  );
}
