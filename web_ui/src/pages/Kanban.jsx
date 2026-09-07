import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { ArrowLeft, ArrowRight, Bot, LogOut, Plus, Trash2, Workflow, X } from "lucide-react";
import {
  clear_token,
  create_card,
  create_project,
  create_workspace,
  fetch_agents,
  fetch_cards,
  fetch_pipelines,
  fetch_projects,
  fetch_workspaces,
  move_card,
  remove_card,
  set_agent,
  set_card_pipeline,
} from "../lib.js";

const COLUMNS = [
  { id: "todo", title: "To Do" },
  { id: "doing", title: "Doing" },
  { id: "done", title: "Done" },
];

const PRIORITIES = ["low", "normal", "high", "critical"];

export default function Kanban() {
  const nav = useNavigate();
  const [workspaces, setWorkspaces] = useState([]);
  const [wsId, setWsId] = useState(null);
  const [projects, setProjects] = useState([]);
  const [projectId, setProjectId] = useState(null);
  const [cards, setCards] = useState([]);
  const [error, setError] = useState("");
  const [title, setTitle] = useState("");
  const [priority, setPriority] = useState(PRIORITIES[1]);
  const [agentFor, setAgentFor] = useState(null);
  const [agentName, setAgentName] = useState("");
  const [agentState, setAgentState] = useState("{}");
  const [savedAgents, setSavedAgents] = useState([]);
  const [pipeFor, setPipeFor] = useState(null);
  const [pipePick, setPipePick] = useState("");
  const [pipelines, setPipelines] = useState(null);

  useEffect(() => {
    load_workspaces();
  }, []);

  async function handle(err) {
    if (err.status === 401) {
      clear_token();
      nav("/login");
      return;
    }
    setError(String(err.message || err));
  }

  async function load_workspaces() {
    try {
      const list = await fetch_workspaces();
      setWorkspaces(list);
      if (list.length > 0) {
        setWsId((cur) => cur ?? list[0].id);
      }
    } catch (err) {
      handle(err);
    }
  }

  useEffect(() => {
    if (wsId == null) return;
    load_projects();
  }, [wsId]);

  async function load_projects() {
    try {
      const list = await fetch_projects(wsId);
      setProjects(list);
      setProjectId((cur) => (list.some((p) => p.id === cur) ? cur : list[0]?.id ?? null));
    } catch (err) {
      handle(err);
    }
  }

  useEffect(() => {
    if (projectId == null) {
      setCards([]);
      return;
    }
    refresh();
  }, [projectId]);

  async function refresh() {
    try {
      setCards(await fetch_cards(projectId));
    } catch (err) {
      handle(err);
    }
  }

  function pick_workspace(id) {
    setWsId(Number(id));
    setProjectId(null);
  }

  async function add_workspace() {
    const name = window.prompt("workspace name");
    if (!name?.trim()) return;
    setError("");
    try {
      await create_workspace(name.trim());
      await load_workspaces();
    } catch (err) {
      handle(err);
    }
  }

  async function add_project() {
    const name = window.prompt("project name");
    if (!name?.trim()) return;
    setError("");
    try {
      await create_project(wsId, name.trim());
      await load_projects();
    } catch (err) {
      handle(err);
    }
  }

  async function add(e) {
    e.preventDefault();
    if (!title.trim() || projectId == null) return;
    setError("");
    try {
      await create_card(projectId, COLUMNS[0].id, title.trim(), "", priority);
      setTitle("");
      await refresh();
    } catch (err) {
      handle(err);
    }
  }

  async function shift(card, dir) {
    const idx = COLUMNS.findIndex((c) => c.id === card.column_id);
    const next = COLUMNS[idx + dir];
    if (!next) return;
    setError("");
    try {
      await move_card(card.id, next.id, 0);
      await refresh();
    } catch (err) {
      handle(err);
    }
  }

  async function drop(card, column_id) {
    if (card.column_id === column_id) return;
    setError("");
    try {
      await move_card(card.id, column_id, 0);
      await refresh();
    } catch (err) {
      handle(err);
    }
  }

  async function del(id) {
    setError("");
    try {
      await remove_card(id);
      await refresh();
    } catch (err) {
      handle(err);
    }
  }

  function open_agent_modal(card) {
    setAgentFor(card);
    setAgentName(card.agent_name || "");
    setAgentState(
      card.agent_state ? JSON.stringify(card.agent_state, null, 2) : "{}"
    );
    fetch_agents().then(setSavedAgents).catch(() => setSavedAgents([]));
  }

  function load_saved_agent(id) {
    const a = savedAgents.find((x) => x.id === Number(id));
    if (!a) return;
    setAgentName(a.name || agentName);
    setAgentState(
      JSON.stringify(
        { model: a.model, persona: a.persona, prompt: a.prompt, output: a.output },
        null,
        2
      )
    );
  }

  function open_pipe_modal(card) {
    setPipeFor(card);
    setPipePick(card.pipeline_id != null ? String(card.pipeline_id) : "");
    if (pipelines == null) {
      fetch_pipelines().then(setPipelines).catch(() => setPipelines([]));
    }
  }

  async function save_pipeline(e) {
    e.preventDefault();
    setError("");
    try {
      await set_card_pipeline(pipeFor.id, pipePick === "" ? null : Number(pipePick));
      setPipeFor(null);
      await refresh();
    } catch (err) {
      handle(err);
    }
  }

  async function save_agent(e) {
    e.preventDefault();
    setError("");
    let state;
    try {
      state = JSON.parse(agentState);
    } catch {
      setError("agent state must be valid JSON");
      return;
    }
    try {
      await set_agent(agentFor.id, agentName.trim(), state);
      setAgentFor(null);
      await refresh();
    } catch (err) {
      handle(err);
    }
  }

  const byColumn = Object.fromEntries(
    COLUMNS.map((c) => [c.id, cards.filter((k) => k.column_id === c.id)])
  );

  return (
    <main className="chat kanban-page">
      <header>
        <h1><Link to="/">kanban</Link></h1>
        <span className="sub">{cards.length} task{cards.length === 1 ? "" : "s"}</span>
        <nav className="nav">
          <select
            className="kanban-select"
            value={wsId ?? ""}
            onChange={(e) => pick_workspace(e.target.value)}
            title="workspace"
          >
            {workspaces.length === 0 && <option value="">no workspace</option>}
            {workspaces.map((w) => (
              <option key={w.id} value={w.id}>{w.name}</option>
            ))}
          </select>
          <button className="kanban-mini" onClick={add_workspace} title="new workspace">+</button>
          <select
            className="kanban-select"
            value={projectId ?? ""}
            onChange={(e) => setProjectId(e.target.value ? Number(e.target.value) : null)}
            title="project"
          >
            {projects.length === 0 && <option value="">no project</option>}
            {projects.map((p) => (
              <option key={p.id} value={p.id}>{p.name}</option>
            ))}
          </select>
          <button className="kanban-mini" onClick={add_project} disabled={wsId == null} title="new project">+</button>
          <Link to="/pipelines" title="pipelines"><Workflow size={16} /></Link>
          <Link to="/agents" title="saved agents"><Bot size={16} /></Link>
          <Link to="/login" title="logout / switch user"><LogOut size={16} /></Link>
          <Link to="/" title="back to chat"><ArrowLeft size={16} /></Link>
        </nav>
      </header>
      {error && <p className="error">{error}</p>}
      <form className="kanban-add" onSubmit={add}>
        <input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder={projectId == null ? "create a workspace + project first…" : "new task title…"}
          disabled={projectId == null}
        />
        <select value={priority} onChange={(e) => setPriority(e.target.value)}>
          {PRIORITIES.map((p) => (
            <option key={p} value={p}>{p}</option>
          ))}
        </select>
        <button type="submit" disabled={!title.trim()}>
          <Plus size={14} /> add
        </button>
      </form>
      <section className="kanban">
        {COLUMNS.map((col) => (
          <div
            key={col.id}
            className="kanban-col"
            onDragOver={(e) => e.preventDefault()}
            onDrop={(e) => {
              const id = Number(e.dataTransfer.getData("text/plain"));
              const card = cards.find((k) => k.id === id);
              if (card) drop(card, col.id);
            }}
          >
            <h2>{col.title} <span className="sub">{byColumn[col.id].length}</span></h2>
            {byColumn[col.id].map((card) => (
              <div
                key={card.id}
                className="kanban-card"
                draggable
                onDragStart={(e) => e.dataTransfer.setData("text/plain", card.id)}
              >
                <div className="kanban-card-top">
                  <span className={`kanban-prio prio-${card.priority}`}>{card.priority}</span>
                  <span className="kanban-card-actions">
                    <button
                      onClick={() => open_pipe_modal(card)}
                      title="assign pipeline"
                      className={card.pipeline_id != null ? "agent-set" : ""}
                    >
                      <Workflow size={13} />
                    </button>
                    <button
                      onClick={() => open_agent_modal(card)}
                      title="attach agent state"
                      className={card.agent_name ? "agent-set" : ""}
                    >
                      <Bot size={13} />
                    </button>
                    <button onClick={() => del(card.id)} title="delete card">
                      <Trash2 size={13} />
                    </button>
                  </span>
                </div>
                <span className="kanban-title">{card.title}</span>
                {card.agent_name && (
                  <span className="kanban-agent">
                    <Bot size={11} /> {card.agent_name}
                  </span>
                )}
                {card.pipeline_name && (
                  <span className="kanban-agent">
                    <Workflow size={11} /> {card.pipeline_name}
                  </span>
                )}
                <span className="kanban-move">
                  <button onClick={() => shift(card, -1)} disabled={card.column_id === COLUMNS[0].id}>
                    <ArrowLeft size={13} />
                  </button>
                  <button onClick={() => shift(card, 1)} disabled={card.column_id === COLUMNS[COLUMNS.length - 1].id}>
                    <ArrowRight size={13} />
                  </button>
                </span>
              </div>
            ))}
          </div>
        ))}
      </section>
      {agentFor && (
        <div className="kanban-modal" onClick={() => setAgentFor(null)}>
          <form className="kanban-modal-box" onClick={(e) => e.stopPropagation()} onSubmit={save_agent}>
            <header>
              <h2><Bot size={14} /> agent on card #{agentFor.id}</h2>
              <button type="button" onClick={() => setAgentFor(null)} title="close"><X size={14} /></button>
            </header>
            <select
              className="kanban-select"
              value=""
              onChange={(e) => load_saved_agent(e.target.value)}
              title="load saved agent"
            >
              <option value="">load saved agent…</option>
              {savedAgents.map((a) => (
                <option key={a.id} value={a.id}>{a.name}</option>
              ))}
            </select>
            <input
              value={agentName}
              onChange={(e) => setAgentName(e.target.value)}
              placeholder="agent name (e.g. qwen-agent)"
            />
            <textarea
              value={agentState}
              onChange={(e) => setAgentState(e.target.value)}
              rows={6}
              spellCheck={false}
            />
            <button type="submit" disabled={!agentName.trim()}>save agent state</button>
          </form>
        </div>
      )}
      {pipeFor && (
        <div className="kanban-modal" onClick={() => setPipeFor(null)}>
          <form className="kanban-modal-box" onClick={(e) => e.stopPropagation()} onSubmit={save_pipeline}>
            <header>
              <h2><Workflow size={14} /> pipeline on card #{pipeFor.id}</h2>
              <button type="button" onClick={() => setPipeFor(null)} title="close"><X size={14} /></button>
            </header>
            <select
              className="kanban-select pipeline-pick"
              value={pipePick}
              onChange={(e) => setPipePick(e.target.value)}
            >
              <option value="">no pipeline</option>
              {(pipelines || []).map((p) => (
                <option key={p.id} value={p.id}>{p.name}</option>
              ))}
            </select>
            <button type="submit">save</button>
          </form>
        </div>
      )}
    </main>
  );
}
