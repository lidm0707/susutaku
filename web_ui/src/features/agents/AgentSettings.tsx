import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  Bot,
  ChevronDown,
  Cpu,
  FileText,
  Play,
  Plus,
  Save,
  Sparkles,
  Terminal,
  Trash2,
} from "lucide-react";
import {
  API_BASE,
  attach_agent_skill,
  chat_codex,
  chat_zai,
  clear_token,
  create_agent,
  create_skill,
  detach_agent_skill,
  fetch_agent_skills,
  fetch_agents,
  fetch_codex_models,
  fetch_models,
  fetch_skills,
  fetch_system_prompt,
  fetch_zai_settings,
  pretty_name,
  remove_agent,
  remove_skill,
  select_model,
  render_prompt,
  save_system_prompt,
  update_agent,
  update_skill,
  type Agent,
  type ChatReply,
  type ModelInfo,
  type RenderedPrompt,
  type Skill,
  type ZaiModel,
} from "../../lib.js";
import { Button, Field, Select, TextArea, TextInput } from "../../ui/controls.js";
import { PromptModal } from "../../ui/Overlay.js";

const CUSTOM = "__custom__";

const OUTPUT_TEMPLATES: Record<string, string> = {
  text: "plain text, concise paragraphs",
  markdown: "## Summary\n\n- point one\n- point two",
  json: '{\n  "key": "value"\n}',
  table: "| column | column |\n|--------|--------|\n| cell   | cell   |",
  code: "```lang\n// code only, no prose\n```",
  list: "1. first\n2. second\n3. third",
};

const OUTPUT_TYPES = Object.keys(OUTPUT_TEMPLATES);

const EMPTY = { name: "", model: "", persona: "", prompt: "", output: "" };

type Fields = typeof EMPTY;

export default function AgentSettings() {
  const nav = useNavigate();
  const [agents, setAgents] = useState<Agent[]>([]);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [codexModels, setCodexModels] = useState<{ id: string; label: string }[]>([]);
  const [zaiModels, setZaiModels] = useState<ZaiModel[]>([]);
  const [selected, setSelected] = useState<number | "new" | null>(null);
  const [fields, setFields] = useState<Fields>(EMPTY);
  const [, setError] = useState("");
  const [status, setStatus] = useState("");
  const [message, setMessage] = useState("");
  const [reply, setReply] = useState<ChatReply | null>(null);
  const [preview, setPreview] = useState<RenderedPrompt | null>(null);
  const [testError, setTestError] = useState("");
  const [busy, setBusy] = useState(false);
  const [sysPrompt, setSysPrompt] = useState("");
  const [sysStatus, setSysStatus] = useState("");
  const [sysOpen, setSysOpen] = useState(false);
  const [newOpen, setNewOpen] = useState(false);
  const [skills, setSkills] = useState<Skill[]>([]);
  const [agentSkills, setAgentSkills] = useState<Skill[]>([]);
  const [skillEdit, setSkillEdit] = useState<Skill | null>(null);
  const [skillNewOpen, setSkillNewOpen] = useState(false);

  function load_skills() {
    fetch_skills()
      .then(setSkills)
      .catch(() => setSkills([]));
  }

  function load_agent_skills(id: number) {
    fetch_agent_skills(id)
      .then(setAgentSkills)
      .catch(() => setAgentSkills([]));
  }

  useEffect(() => {
    load_skills();
  }, []);

  useEffect(() => {
    Promise.all([fetch_agents(), fetch_models().catch(() => [])])
      .then(([a, m]) => {
        setAgents(a);
        setModels(m);
      })
      .catch((err: unknown) => handle(err));
    fetch_codex_models()
      .then((list) =>
        setCodexModels([...new Map(list.map((m) => [m.id, m])).values()])
      )
      .catch(() => setCodexModels([]));
    fetch_zai_settings()
      .then((s) => setZaiModels(s.models))
      .catch(() => setZaiModels([]));
    fetch_system_prompt()
      .then(setSysPrompt)
      .catch(() => setSysPrompt(""));
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

  function pick_model(v: string) {
    if (v === CUSTOM) {
      set("model", "");
    } else {
      set("model", v);
    }
  }

  function pick_output_type(v: string) {
    set("output", OUTPUT_TEMPLATES[v] ?? "");
  }

  function output_type(): string {
    for (const t of OUTPUT_TYPES) {
      if (OUTPUT_TEMPLATES[t] === fields.output) return t;
    }
    return "custom";
  }

  function prompt_sections() {
    const skill_body = agentSkills
      .map((s) => s.body.trim())
      .filter(Boolean)
      .join("\n\n");
    const instructions = [fields.persona, fields.prompt, skill_body]
      .filter((s) => s.trim())
      .join("\n\n");
    return [
      { role: "system", body: sysPrompt },
      { role: "instructions", body: instructions },
      { role: "context", body: fields.output },
    ].filter((s) => s.body.trim());
  }

  async function save_sys(e: React.FormEvent) {
    e.preventDefault();
    setSysStatus("");
    try {
      setSysPrompt(await save_system_prompt(sysPrompt));
      setSysStatus("saved");
    } catch (err: unknown) {
      handle(err);
    }
  }

  async function preview_prompt() {
    setTestError("");
    setPreview(null);
    try {
      setPreview(await render_prompt(prompt_sections()));
    } catch (err: unknown) {
      setTestError(err instanceof Error ? err.message : String(err));
    }
  }

  async function test() {
    setTestError("");
    setReply(null);
    setBusy(true);
    try {
      const local = models.find((m) => m.name === fields.model);
      if (codexModels.some((m) => m.id === fields.model)) {
        setReply(await chat_codex(message, fields.model));
      } else if (local) {
        if (!local.selected) await select_model(fields.model);
        const res = await fetch(`${API_BASE}/api/chat`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ message }),
        });
        setReply(await res.json());
      } else {
        setReply(await chat_zai(message, fields.model, prompt_sections()));
      }
    } catch (err: unknown) {
      setTestError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  const model_selected =
    codexModels.some((m) => m.id === fields.model) ||
    zaiModels.some((m) => m.model === fields.model) ||
    models.some((m) => m.name === fields.model);

  function pick(a: Agent) {
    setError("");
    setStatus("");
    setSysOpen(false);
    setSkillEdit(null);
    setSelected(a.id);
    setFields({
      name: a.name || "",
      model: a.model || "",
      persona: a.persona || "",
      prompt: a.prompt || "",
      output: a.output || "",
    });
    if (typeof a.id === "number") load_agent_skills(a.id);
    else setAgentSkills([]);
  }

  function start_new(name: string) {
    setError("");
    setStatus("");
    setSysOpen(false);
    setSkillEdit(null);
    setSelected("new");
    setAgentSkills([]);
    setFields({ ...EMPTY, name });
  }

  async function attach_skill(skill_id: number) {
    if (typeof selected !== "number") return;
    try {
      await attach_agent_skill(selected, skill_id);
      load_agent_skills(selected);
    } catch (err) {
      handle(err);
    }
  }

  async function detach_skill(skill_id: number) {
    if (typeof selected !== "number") return;
    try {
      await detach_agent_skill(selected, skill_id);
      load_agent_skills(selected);
    } catch (err) {
      handle(err);
    }
  }

  async function new_skill(name: string) {
    try {
      await create_skill(name.trim(), "");
      load_skills();
    } catch (err) {
      handle(err);
    }
  }

  async function del_skill(id: number) {
    try {
      await remove_skill(id);
      setAgentSkills((cur) => cur.filter((s) => s.id !== id));
      setSkillEdit(null);
      load_skills();
    } catch (err) {
      handle(err);
    }
  }

  async function save_skill(e: React.FormEvent) {
    e.preventDefault();
    if (!skillEdit) return;
    try {
      await update_skill(skillEdit.id, skillEdit.name, skillEdit.body);
      load_skills();
      if (typeof selected === "number") load_agent_skills(selected);
    } catch (err) {
      handle(err);
    }
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
      setFields(EMPTY);
      setSelected(null);
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
          <Button variant="ghost" className="agents-new" onClick={() => setNewOpen(true)}>
            <Plus size={14} /> new agent
          </Button>
          <div className="agents-list">
            <button
              className={sysOpen ? "agent-item active" : "agent-item"}
              onClick={() => {
                setSysOpen(!sysOpen);
                setSelected(null);
                setSkillEdit(null);
              }}
            >
              <Sparkles size={14} />
              <span className="agent-item-name">system prompt</span>
              {!sysPrompt.trim() && <span className="agent-item-model">empty</span>}
            </button>
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
          <div className="agents-list">
            <div className="agent-item" role="presentation">
              <Sparkles size={14} />
              <span className="agent-item-name">skills</span>
              <Button
                variant="ghost"
                type="button"
                onClick={() => setSkillNewOpen(true)}
                title="new skill"
              >
                <Plus size={12} />
              </Button>
            </div>
            {skills.map((s) => (
              <button
                key={s.id}
                className={skillEdit?.id === s.id ? "agent-item active" : "agent-item"}
                onClick={() => setSkillEdit(s)}
              >
                <Terminal size={14} />
                <span className="agent-item-name">{s.name}</span>
              </button>
            ))}
          </div>
        </aside>
        {skillEdit ? (
          <form className="agent-editor" onSubmit={save_skill}>
            <Field label={`skill: ${skillEdit.name}`} icon={<Terminal size={12} />}>
              <TextArea
                className="agent-prompt"
                value={skillEdit.body}
                onChange={(e) => setSkillEdit({ ...skillEdit, body: e.target.value })}
                rows={16}
                spellCheck={false}
                placeholder="skill instructions injected into the agent prompt…"
              />
            </Field>
            <footer className="agent-editor-foot">
              <Button variant="primary" type="submit">
                <Save size={14} /> save
              </Button>
              <Button variant="danger" type="button" onClick={() => del_skill(skillEdit.id)}>
                <Trash2 size={14} /> delete
              </Button>
            </footer>
          </form>
        ) : sysOpen ? (
          <form className="agent-editor" onSubmit={save_sys}>
            <Field label="global system prompt" icon={<Sparkles size={12} />}>
              <TextArea
                className="agent-prompt"
                value={sysPrompt}
                onChange={(e) => {
                  setSysPrompt(e.target.value);
                  setSysStatus("");
                }}
                rows={16}
                spellCheck={false}
                placeholder="global system prompt for the whole system…"
              />
            </Field>
            <footer className="agent-editor-foot">
              <Button variant="primary" type="submit">
                <Save size={14} /> save
              </Button>
              {sysStatus && <span className="saved-mark">{sysStatus}</span>}
            </footer>
          </form>
        ) : editing ? (
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
                <div className="select-wrap">
                  <Select
                    value={model_selected ? fields.model : CUSTOM}
                    onChange={(e) => pick_model(e.target.value)}
                  >
                    {!model_selected && fields.model !== "" && (
                      <option value={fields.model}>{pretty_name(fields.model)}</option>
                    )}
                    <optgroup label="codex (gpt)">
                      {codexModels.map((m) => (
                        <option key={m.id} value={m.id}>{m.label || m.id}</option>
                      ))}
                    </optgroup>
                    {zaiModels.length > 0 && (
                      <optgroup label="external (api)">
                        {zaiModels.map((m) => (
                          <option key={m.model} value={m.model}>{m.model}</option>
                        ))}
                      </optgroup>
                    )}
                    {models.length > 0 && (
                      <optgroup label="local mlx">
                        {models.map((m) => (
                          <option key={m.name} value={m.name}>{pretty_name(m.name)}</option>
                        ))}
                      </optgroup>
                    )}
                    <option value={CUSTOM}>custom…</option>
                  </Select>
                  <ChevronDown size={13} className="select-arrow" />
                </div>
                {!model_selected && (
                  <TextInput
                    value={fields.model}
                    onChange={(e) => set("model", e.target.value)}
                    placeholder="custom model name"
                  />
                )}
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
            <Field label="instruction" icon={<Terminal size={12} />}>
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
              <div className="select-wrap">
                <Select
                  value={output_type()}
                  onChange={(e) => pick_output_type(e.target.value)}
                  aria-label="output type"
                >
                  {OUTPUT_TYPES.map((t) => (
                    <option key={t} value={t}>{t}</option>
                  ))}
                  <option value="custom">custom</option>
                </Select>
                <ChevronDown size={13} className="select-arrow" />
              </div>
              <TextArea
                value={fields.output}
                onChange={(e) => set("output", e.target.value)}
                rows={3}
                spellCheck={false}
                placeholder="what the output should look like…"
              />
            </Field>
            <Field label="skills" icon={<Sparkles size={12} />}>
              {typeof selected === "number" && (
                <>
                  <div className="agent-skill-list">
                    {agentSkills.length === 0 && <span className="agents-empty">no skills attached</span>}
                    {agentSkills.map((s) => (
                      <span key={s.id} className="agent-skill-chip">
                        {s.name}
                        <button
                          type="button"
                          onClick={() => detach_skill(s.id)}
                          title="detach skill"
                        >
                          <Trash2 size={11} />
                        </button>
                      </span>
                    ))}
                  </div>
                  <div className="select-wrap">
                    <Select
                      value=""
                      onChange={(e) => {
                        const id = Number(e.target.value);
                        if (id) attach_skill(id);
                      }}
                      aria-label="attach skill"
                    >
                      <option value="">attach skill…</option>
                      {skills
                        .filter((s) => !agentSkills.some((a) => a.id === s.id))
                        .map((s) => (
                          <option key={s.id} value={s.id}>{s.name}</option>
                        ))}
                    </Select>
                    <ChevronDown size={13} className="select-arrow" />
                  </div>
                </>
              )}
              {typeof selected !== "number" && (
                <span className="agents-empty">save the agent first, then attach skills</span>
              )}
            </Field>
            <div className="prompt-section">
              <TextArea
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                placeholder="test message…"
                rows={3}
              />
              {testError && <p className="error">{testError}</p>}
              {preview && <pre className="prompt-preview">{preview.rendered}</pre>}
              {reply && <pre className="prompt-preview">{reply.reply}</pre>}
              <div className="prompt-section-head">
                <Button variant="ghost" type="button" onClick={preview_prompt}>
                  <FileText size={14} /> render
                </Button>
                <Button
                  variant="primary"
                  type="button"
                  onClick={test}
                  disabled={busy || !message.trim()}
                >
                  <Play size={14} /> {busy ? "…" : "test"}
                </Button>
              </div>
            </div>
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
            <Button variant="ghost" onClick={() => setNewOpen(true)}><Plus size={14} /> new agent</Button>
          </div>
        )}
      </div>
      <PromptModal
        open={newOpen}
        title="new agent"
        placeholder="name…"
        on_close={() => setNewOpen(false)}
        on_submit={(v) => {
          setNewOpen(false);
          start_new(v);
        }}
      />
      <PromptModal
        open={skillNewOpen}
        title="new skill"
        placeholder="skill name…"
        on_close={() => setSkillNewOpen(false)}
        on_submit={(v) => {
          setSkillNewOpen(false);
          new_skill(v);
        }}
      />
    </main>
  );
}
