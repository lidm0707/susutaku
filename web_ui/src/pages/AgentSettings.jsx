import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { ArrowLeft, Bot, Trash2 } from "lucide-react";
import {
  clear_token,
  create_agent,
  fetch_agents,
  fetch_models,
  pretty_name,
  remove_agent,
  update_agent,
} from "../lib.js";

const EMPTY = { name: "", model: "", persona: "", prompt: "", output: "" };

export default function AgentSettings() {
  const nav = useNavigate();
  const [agents, setAgents] = useState([]);
  const [models, setModels] = useState([]);
  const [selected, setSelected] = useState(null);
  const [fields, setFields] = useState(EMPTY);
  const [error, setError] = useState("");
  const [status, setStatus] = useState("");

  useEffect(() => {
    Promise.all([fetch_agents(), fetch_models().catch(() => [])])
      .then(([a, m]) => {
        setAgents(a);
        setModels(m);
      })
      .catch((err) => handle(err));
  }, []);

  async function handle(err) {
    if (err.status === 401) {
      clear_token();
      nav("/login");
      return;
    }
    setError(String(err.message || err));
  }

  function set(k, v) {
    setFields({ ...fields, [k]: v });
  }

  function pick(a) {
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

  async function save(e) {
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
      let row;
      if (selected === "new") {
        row = await create_agent(payload);
      } else {
        await update_agent(selected, payload);
        row = { ...payload, id: selected };
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

  return (
    <main className="chat kanban-page">
      <header>
        <h1><Link to="/agents">agents</Link></h1>
        <span className="sub">{agents.length} saved</span>
        <nav className="nav">
          <Link to="/" title="back to chat"><ArrowLeft size={16} /></Link>
        </nav>
      </header>
      {error && <p className="error">{error}</p>}
      <div className="kanban-add">
        <select
          className="kanban-select"
          value={selected ?? ""}
          onChange={(e) => {
            const a = agents.find((x) => x.id === Number(e.target.value));
            if (a) pick(a);
          }}
          title="saved agent"
        >
          <option value="">no agent selected</option>
          {agents.map((a) => (
            <option key={a.id} value={a.id}>{a.name}</option>
          ))}
        </select>
        <button className="kanban-mini" onClick={start_new} title="new agent"><Bot size={13} /> new agent</button>
        <button className="kanban-mini" onClick={del} disabled={selected == null || selected === "new"} title="delete agent"><Trash2 size={13} /></button>
      </div>
      {selected != null && (
        <form className="settings-form" onSubmit={save}>
          <label>name</label>
          <input value={fields.name} onChange={(e) => set("name", e.target.value)} placeholder="agent name (e.g. qwen-agent)" />
          <label>model</label>
          <input
            list="agent-models"
            value={fields.model}
            onChange={(e) => set("model", e.target.value)}
            placeholder="model name or custom"
          />
          <datalist id="agent-models">
            {models.map((m) => (
              <option key={m.name} value={m.name}>{pretty_name(m.name)}</option>
            ))}
          </datalist>
          <label>persona</label>
          <textarea
            value={fields.persona}
            onChange={(e) => set("persona", e.target.value)}
            rows={3}
            spellCheck={false}
            placeholder="personalize: who the agent is / tone"
          />
          <label>prompt</label>
          <textarea
            value={fields.prompt}
            onChange={(e) => set("prompt", e.target.value)}
            rows={5}
            spellCheck={false}
            placeholder="instruction: what the agent must do"
          />
          <label>output</label>
          <textarea
            value={fields.output}
            onChange={(e) => set("output", e.target.value)}
            rows={3}
            spellCheck={false}
            placeholder="what the output should look like"
          />
          <button type="submit" disabled={!fields.name.trim()}>save agent</button>
          {status && <span className="saved-mark">{status}</span>}
        </form>
      )}
    </main>
  );
}
