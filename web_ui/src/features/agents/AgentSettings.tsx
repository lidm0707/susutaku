import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Bot, Cpu, Plus, Save, Sparkles, Terminal, Trash2 } from "lucide-react";
import {
  clear_token,
  create_agent,
  fetch_agents,
  fetch_codex_models,
  fetch_models,
  pretty_name,
  remove_agent,
  update_agent,
  type Agent,
  type ModelInfo,
} from "../../lib.js";
import { Button, Field, TextArea, TextInput } from "../../ui/controls.js";

const EMPTY = { name: "", model: "", persona: "", prompt: "", output: "" };

type Fields = typeof EMPTY;

export default function AgentSettings() {
  const nav = useNavigate();
  const [agents, setAgents] = useState<Agent[]>([]);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [codexModels, setCodexModels] = useState<string[]>([]);
  const [selected, setSelected] = useState<number | "new" | null>(null);
  const [fields, setFields] = useState<Fields>(EMPTY);
  const [, setError] = useState("");
  const [status, setStatus] = useState("");

  useEffect(() => {
    Promise.all([fetch_agents(), fetch_models().catch(() => [])])
      .then(([a, m]) => {
        setAgents(a);
        setModels(m);
      })
      .catch((err: unknown) => handle(err));
    fetch_codex_models()
      .then((list) => setCodexModels(list.map((m) => m.id)))
      .catch(() => setCodexModels([]));
  }, []);

  async function handle(err: unknown) {
    if (err instanceof Object && "status" in err && (err as { status?: number }).status === 401) {
      clear_token();
      nav("/");
      return;
    }
    setError(err instanceof Error ? err.message : String(err));
  }

  function set(k: keyof Fields, v: string) {
    setFields({ ...fields, [k]: v });
  }

  function pick(a: Agent) {
    setError("");
    setStatus("");
    setSelected(a.id);
    setFields({
      name: a.name || "",
      model: a.model || "",
      persona: a.persona || "",
      prompt: a.prompt || "",
      output: a.output || "",
    });
  }

  function start_new() {
    setError("");
    setStatus("");
    setSelected("new");
    setFields(EMPTY);
  }

  async function save(e: React.FormEvent) {
    e.preventDefault();
    setStatus("");
    const payload = {
      name: fields.name.trim(),
      model: fields.model.trim(),
      persona: fields.persona,
      prompt: fields.prompt,
      output: fields.output,
    };
    setError("");
    try {
      let row: Agent;
      if (selected === "new") {
        row = await create_agent(payload);
      } else {
        await update_agent(selected as number, payload);
        row = { ...payload, id: selected as number };
      }
      setSelected(row.id);
      setStatus("saved");
      await load_agents();
    } catch (err) {
      handle(err);
    }
  }

  async function load_agents() {
    try {
      setAgents(await fetch_agents());
    } catch (err) {
      handle(err);
    }
  }

  async function del() {
    if (selected == null || selected === "new") return;
    setError("");
    try {
      await remove_agent(selected);
      start_new();
      await load_agents();
    } catch (err) {
      handle(err);
    }
  }

  const editing = selected != null;

  return (
    <main className="chat kanban-page agents-page">
      <header>
        <h1>agents</h1>
        <span className="sub">{agents.length} saved</span>
      </header>
      <div className="agents-layout">
        <aside className="agents-side">
          <Button variant="ghost" className="agents-new" onClick={start_new}>
            <Plus size={14} /> new agent
          </Button>
          <div className="agents-list">
            {agents.length === 0 && <span className="agents-empty">no agents yet</span>}
            {agents.map((a) => (
              <button
                key={a.id}
                className={selected === a.id ? "agent-item active" : "agent-item"}
                onClick={() => pick(a)}
              >
                <Bot size={14} />
                <span className="agent-item-name">{a.name}</span>
                {a.model && <span className="agent-item-model">{pretty_name(a.model)}</span>}
              </button>
            ))}
          </div>
        </aside>
        {editing ? (
          <form className="agent-editor" onSubmit={save}>
            <div className="agent-editor-row">
              <Field label="name" icon={<Bot size={12} />}>
                <TextInput
                  value={fields.name}
                  onChange={(e) => set("name", e.target.value)}
                  placeholder="agent name (e.g. qwen-agent)"
                />
              </Field>
              <Field label="model" icon={<Cpu size={12} />}>
                <TextInput
                  list="agent-models"
                  value={fields.model}
                  onChange={(e) => set("model", e.target.value)}
                  placeholder="model name or custom"
                />
                <datalist id="agent-models">
                  {codexModels.map((id) => (
                    <option key={id} value={id}>codex (gpt)</option>
                  ))}
                  {models.map((m) => (
                    <option key={m.name} value={m.name}>{pretty_name(m.name)}</option>
                  ))}
                </datalist>
              </Field>
            </div>
            <Field label="persona" icon={<Sparkles size={12} />}>
              <TextArea
                value={fields.persona}
                onChange={(e) => set("persona", e.target.value)}
                rows={3}
                spellCheck={false}
                placeholder="who the agent is / tone…"
              />
            </Field>
            <Field label="prompt" icon={<Terminal size={12} />}>
              <TextArea
                className="agent-prompt"
                value={fields.prompt}
                onChange={(e) => set("prompt", e.target.value)}
                rows={8}
                spellCheck={false}
                placeholder="instruction: what the agent must do…"
              />
            </Field>
            <Field label="output" icon={<Sparkles size={12} />}>
              <TextArea
                value={fields.output}
                onChange={(e) => set("output", e.target.value)}
                rows={3}
                spellCheck={false}
                placeholder="what the output should look like…"
              />
            </Field>
            <footer className="agent-editor-foot">
              <Button variant="primary" type="submit" disabled={!fields.name.trim()}>
                <Save size={14} /> save agent
              </Button>
              {selected !== "new" && (
                <Button variant="danger" type="button" onClick={del} title="delete agent">
                  <Trash2 size={14} /> delete
                </Button>
              )}
              {status && <span className="saved-mark">{status}</span>}
            </footer>
          </form>
        ) : (
          <div className="agent-editor agent-editor-empty">
            <Bot size={28} />
            <p>select an agent on the left,<br />or create a new one.</p>
            <Button variant="ghost" onClick={start_new}><Plus size={14} /> new agent</Button>
          </div>
        )}
      </div>
    </main>
  );
}
