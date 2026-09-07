import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { ArrowLeft, ArrowRight, Bot, LogOut, Plus, Trash2, User, Workflow } from "lucide-react";
import {
  clear_token,
  add_comment,
  create_card,
  create_project,
  create_workspace,
  fetch_agents,
  fetch_cards,
  fetch_comments,
  fetch_pipelines,
  fetch_projects,
  fetch_workspaces,
  move_card,
  remove_card,
  set_agent,
  set_card_pipeline,
  update_card,
  type Agent,
  type Card,
  type Comment,
  type Pipeline,
  type Project,
  type Workspace,
} from "../lib.js";
import { Modal, PromptModal, SlideOver } from "../ui/Overlay.js";

const COLUMNS = [
  { id: "todo", title: "To Do" },
  { id: "doing", title: "Doing" },
  { id: "done", title: "Done" },
] as const;

const PRIORITIES = ["low", "normal", "high", "critical"] as const;

type ColumnId = (typeof COLUMNS)[number]["id"];
type Priority = (typeof PRIORITIES)[number];

export default function Kanban() {
  const nav = useNavigate();
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [wsId, setWsId] = useState<number | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [projectId, setProjectId] = useState<number | null>(null);
  const [cards, setCards] = useState<Card[]>([]);
  const [, setError] = useState("");
  const [title, setTitle] = useState("");
  const [priority, setPriority] = useState<Priority>(PRIORITIES[1]);
  const [agentFor, setAgentFor] = useState<Card | null>(null);
  const [agentName, setAgentName] = useState("");
  const [agentState, setAgentState] = useState("{}");
  const [savedAgents, setSavedAgents] = useState<Agent[]>([]);
  const [pipeFor, setPipeFor] = useState<Card | null>(null);
  const [pipePick, setPipePick] = useState("");
  const [pipelines, setPipelines] = useState<Pipeline[] | null>(null);
  const [detail, setDetail] = useState<Card | null>(null);
  const [dTitle, setDTitle] = useState("");
  const [dDesc, setDDesc] = useState("");
  const [dAssignee, setDAssignee] = useState("");
  const [dComments, setDComments] = useState<Comment[]>([]);
  const [dCommentBody, setDCommentBody] = useState("");
  const [prompting, setPrompting] = useState<null | "workspace" | "project">(null);
  const [dragOver, setDragOver] = useState<ColumnId | null>(null);
  const [draggingId, setDraggingId] = useState<number | null>(null);

  useEffect(() => {
    load_workspaces();
  }, []);

  async function handle(err: unknown) {
    if (err instanceof Object && "status" in err && (err as { status?: number }).status === 401) {
      clear_token();
      nav("/");
      return;
    }
    setError(err instanceof Error ? err.message : String(err));
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
      const list = await fetch_projects(wsId as number);
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
      setCards(await fetch_cards(projectId as number));
    } catch (err) {
      handle(err);
    }
  }

  function pick_workspace(id: string) {
    setWsId(Number(id));
    setProjectId(null);
  }

  async function add_workspace(name: string) {
    setPrompting(null);
    setError("");
    try {
      await create_workspace(name);
      await load_workspaces();
    } catch (err) {
      handle(err);
    }
  }

  async function add_project(name: string) {
    setPrompting(null);
    setError("");
    try {
      await create_project(wsId as number, name);
      await load_projects();
    } catch (err) {
      handle(err);
    }
  }

  async function add(e: React.FormEvent) {
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

  async function shift(card: Card, dir: number) {
    const idx = COLUMNS.findIndex((c) => c.id === card.column_id);
    const next: ColumnId | undefined = COLUMNS[idx + dir]?.id;
    if (!next) return;
    setError("");
    try {
      await move_card(card.id, next, 0);
      await refresh();
    } catch (err) {
      handle(err);
    }
  }

  async function drop(card: Card, column_id: ColumnId) {
    if (card.column_id === column_id) return;
    setError("");
    try {
      await move_card(card.id, column_id, 0);
      await refresh();
    } catch (err) {
      handle(err);
    }
  }

  async function del(id: number) {
    setError("");
    try {
      await remove_card(id);
      await refresh();
    } catch (err) {
      handle(err);
    }
  }

  function open_agent_modal(card: Card) {
    setAgentFor(card);
    setAgentName(card.agent_name || "");
    setAgentState(
      card.agent_state ? JSON.stringify(card.agent_state, null, 2) : "{}"
    );
    fetch_agents().then(setSavedAgents).catch(() => setSavedAgents([]));
  }

  async function open_detail(card: Card) {
    setDetail(card);
    setDTitle(card.title);
    setDDesc(card.description);
    setDAssignee(card.assignee || "");
    setDComments([]);
    setDCommentBody("");
    fetch_agents().then(setSavedAgents).catch(() => setSavedAgents([]));
    try {
      setDComments(await fetch_comments(card.id));
    } catch (err) {
      handle(err);
    }
  }

  async function save_detail(e: React.FormEvent) {
    e.preventDefault();
    if (!detail || !dTitle.trim()) return;
    setError("");
    try {
      await update_card(detail.id, dTitle.trim(), dDesc, dAssignee.trim() || null);
      setDetail(null);
      await refresh();
    } catch (err) {
      handle(err);
    }
  }

  async function comment(e: React.FormEvent) {
    e.preventDefault();
    if (!detail || !dCommentBody.trim()) return;
    setError("");
    try {
      await add_comment(detail.id, dCommentBody.trim());
      setDCommentBody("");
      setDComments(await fetch_comments(detail.id));
    } catch (err) {
      handle(err);
    }
  }

  function pick_detail_bot(id: string) {
    const a = savedAgents.find((x) => x.id === Number(id));
    if (!a || !detail) return;
    setError("");
    set_agent(
      detail.id,
      a.name,
      { model: a.model, persona: a.persona, prompt: a.prompt, output: a.output }
    )
      .then(refresh)
      .then(() => setDetail(null))
      .catch(handle);
  }

  function load_saved_agent(id: string) {
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

  function open_pipe_modal(card: Card) {
    setPipeFor(card);
    setPipePick(card.pipeline_id != null ? String(card.pipeline_id) : "");
    if (pipelines == null) {
      fetch_pipelines().then(setPipelines).catch(() => setPipelines([]));
    }
  }

  async function save_pipeline(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    try {
      await set_card_pipeline((pipeFor as Card).id, pipePick === "" ? null : Number(pipePick));
      setPipeFor(null);
      await refresh();
    } catch (err) {
      handle(err);
    }
  }

  async function save_agent(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    let state: unknown;
    try {
      state = JSON.parse(agentState);
    } catch {
      setError("agent state must be valid JSON");
      return;
    }
    try {
      await set_agent((agentFor as Card).id, agentName.trim(), state);
      setAgentFor(null);
      await refresh();
    } catch (err) {
      handle(err);
    }
  }

  const byColumn = Object.fromEntries(
    COLUMNS.map((c) => [c.id, cards.filter((k) => k.column_id === c.id)])
  ) as Record<ColumnId, Card[]>;

  return (
    <main className="chat kanban-page">
      <header>
        <h1><Link to="/chat">kanban</Link></h1>
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
          <button className="kanban-mini" onClick={() => setPrompting("workspace")} title="new workspace">+</button>
          <select
            className="kanban-select"
            value={projectId ?? ""}
            onChange={(e: React.ChangeEvent<HTMLSelectElement>) => setProjectId(e.target.value ? Number(e.target.value) : null)}
            title="project"
          >
            {projects.length === 0 && <option value="">no project</option>}
            {projects.map((p) => (
              <option key={p.id} value={p.id}>{p.name}</option>
            ))}
          </select>
          <button className="kanban-mini" onClick={() => setPrompting("project")} disabled={wsId == null} title="new project">+</button>
          <Link to="/pipelines" title="pipelines"><Workflow size={16} /></Link>
          <Link to="/agents" title="saved agents"><Bot size={16} /></Link>
          <Link to="/" title="logout / switch user"><LogOut size={16} /></Link>
          <Link to="/chat" title="back to chat"><ArrowLeft size={16} /></Link>
        </nav>
      </header>
      <form className="kanban-add" onSubmit={add}>
        <input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder={projectId == null ? "create a workspace + project first…" : "new task title…"}
          disabled={projectId == null}
        />
        <select value={priority} onChange={(e) => setPriority(e.target.value as Priority)}>
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
            className={dragOver === col.id ? "kanban-col drag-over" : "kanban-col"}
            onDragOver={(e: React.DragEvent<HTMLDivElement>) => {
              e.preventDefault();
              setDragOver(col.id);
            }}
            onDragLeave={() => setDragOver((cur) => (cur === col.id ? null : cur))}
            onDrop={(e: React.DragEvent<HTMLDivElement>) => {
              e.preventDefault();
              setDragOver(null);
              setDraggingId(null);
              const id = Number(e.dataTransfer.getData("text/plain"));
              const card = cards.find((k) => k.id === id);
              if (card) drop(card, col.id);
            }}
          >
            <h2>{col.title} <span className="sub">{byColumn[col.id].length}</span></h2>
            {byColumn[col.id].map((card) => (
              <div
                key={card.id}
                className={draggingId === card.id ? "kanban-card dragging" : "kanban-card"}
                draggable
                onDragStart={(e: React.DragEvent<HTMLDivElement>) => {
                  e.dataTransfer.setData("text/plain", String(card.id));
                  setDraggingId(card.id);
                }}
                onDragEnd={() => setDraggingId(null)}
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
                <span className="kanban-title kanban-title-link" onClick={() => open_detail(card)} title="open card">
                  {card.title}
                </span>
                {card.description && (
                  <span className="kanban-desc" title={card.description}>
                    {card.description}
                  </span>
                )}
                {card.agent_name && (
                  <span className="kanban-agent">
                    <Bot size={11} /> {card.agent_name}
                  </span>
                )}
                {card.assignee && (
                  <span className="kanban-agent">
                    <User size={11} /> {card.assignee}
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
      <PromptModal
        open={prompting != null}
        title={prompting === "project" ? "new project" : "new workspace"}
        placeholder="name…"
        on_close={() => setPrompting(null)}
        on_submit={(v) => (prompting === "project" ? add_project(v) : add_workspace(v))}
      />
      <Modal
        open={agentFor != null}
        title={<><Bot size={14} /> agent on card #{agentFor?.id}</>}
        on_close={() => setAgentFor(null)}
      >
        <form className="modal-form" onSubmit={save_agent}>
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
      </Modal>
      <Modal
        open={pipeFor != null}
        title={<><Workflow size={14} /> pipeline on card #{pipeFor?.id}</>}
        on_close={() => setPipeFor(null)}
      >
        <form className="modal-form" onSubmit={save_pipeline}>
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
      </Modal>
      <SlideOver
        open={detail != null}
        title={<>card #{detail?.id}</>}
        on_close={() => setDetail(null)}
      >
            <form className="kanban-detail-form" onSubmit={save_detail}>
              <input
                value={dTitle}
                onChange={(e) => setDTitle(e.target.value)}
                placeholder="title"
              />
              <textarea
                value={dDesc}
                onChange={(e) => setDDesc(e.target.value)}
                rows={4}
                spellCheck={false}
                placeholder="description…"
              />
              <div className="kanban-assign-row">
                <User size={13} />
                <input
                  className="kanban-select"
                  value={dAssignee}
                  onChange={(e) => setDAssignee(e.target.value)}
                  placeholder="assign person…"
                />
                <select
                  className="kanban-select"
                  value=""
                  onChange={(e) => pick_detail_bot(e.target.value)}
                  title="assign bot (saved agent)"
                >
                  <option value="">assign bot…</option>
                  {savedAgents.map((a) => (
                    <option key={a.id} value={a.id}>{a.name}</option>
                  ))}
                </select>
              </div>
              <button type="submit" disabled={!dTitle.trim()}>save</button>
            </form>
            <div className="kanban-comments">
              {dComments.map((c) => (
                <div key={c.id} className="kanban-comment">
                  <span className="kanban-comment-meta">
                    <strong>{c.author}</strong> {new Date(c.created_at).toLocaleString()}
                  </span>
                  <span>{c.body}</span>
                </div>
              ))}
              <form className="kanban-add" onSubmit={comment}>
                <input
                  value={dCommentBody}
                  onChange={(e) => setDCommentBody(e.target.value)}
                  placeholder="write a comment…"
                />
                <button type="submit" disabled={!dCommentBody.trim()}>
                  <Plus size={14} />
                </button>
              </form>
            </div>
      </SlideOver>
    </main>
  );
}
