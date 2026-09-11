import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { ArrowLeft, ArrowRight, Bot, CalendarClock, CheckCircle2, Clock, Eye, EyeOff, Flag, Hash, LayoutGrid, List, ListChecks, Loader2, Play, Plus, Save, Tag, Trash2, User, Workflow, X, XCircle, Zap } from "lucide-react";
import {
  clear_token,
  add_comment,
  chat,
  create_card,
  fetch_agents,
  fetch_cards,
  fetch_comments,
  fetch_cronjobs,
  fetch_pipelines,
  fetch_users,
  move_card,
  remove_card,
  set_agent,
  set_card_pipeline,
  set_card_schedule,
  run_card,
  run_of,
  update_card,
  type Agent,
  type Card,
  type CardRun,
  type ChecklistItem,
  type Comment,
  type CronJob,
  type Pipeline,
  type UserInfo,
} from "../lib.js";
import { Modal, SlideOver } from "../ui/Overlay.js";
import { toast } from "../ui/Toast.js";
import { use_projects } from "../components/ProjectContext.tsx";

const COLUMNS = [
  { id: "todo", title: "To Do" },
  { id: "doing", title: "Doing" },
  { id: "done", title: "Done" },
] as const;

const PRIORITIES = ["low", "normal", "high", "critical"] as const;

const CRON_PRESETS = [
  { expr: "* * * * *", label: "every minute" },
  { expr: "*/5 * * * *", label: "every 5 minutes" },
  { expr: "*/15 * * * *", label: "every 15 minutes" },
  { expr: "0 * * * *", label: "hourly" },
  { expr: "0 */6 * * *", label: "every 6 hours" },
  { expr: "0 3 * * *", label: "daily 03:00" },
] as const;

type ColumnId = (typeof COLUMNS)[number]["id"];
type Priority = (typeof PRIORITIES)[number];

const VIEW_BOARD = "board";
const VIEW_LIST = "list";
type ViewMode = typeof VIEW_BOARD | typeof VIEW_LIST;

const LABEL_PALETTE = ["#7bd88f", "#e3b341", "#ff7b7b", "#6fb3ff", "#c792ea", "#64d8cb"];
const DUE_SOON_DAYS = 2;
const ESTIMATE_MAX = 99;

type DueState = "overdue" | "soon" | null;

function cron_label(expr: string | null | undefined): string {
  if (!expr) return "";
  return CRON_PRESETS.find((p) => p.expr === expr)?.label || expr;
}

function fmt_time(epoch: number): string {
  return new Date(epoch * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function next_run_for(card_id: number, jobs: CronJob[]): number | null {
  return jobs.find((j) => j.card_id === card_id)?.next_run ?? null;
}

function parse_labels(card: Card): string[] {
  if (!card.labels) return [];
  try {
    const v: unknown = JSON.parse(card.labels);
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

function parse_checklist(card: Card): ChecklistItem[] {
  if (!card.checklist) return [];
  try {
    const v: unknown = JSON.parse(card.checklist);
    if (!Array.isArray(v)) return [];
    return v.filter(
      (x): x is ChecklistItem =>
        x instanceof Object && typeof (x as ChecklistItem).text === "string"
    );
  } catch {
    return [];
  }
}

function label_color(name: string): string {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  return LABEL_PALETTE[h % LABEL_PALETTE.length];
}

function due_state(deadline: string | null | undefined): DueState {
  if (!deadline) return null;
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const due = new Date(`${deadline}T00:00:00`);
  if (Number.isNaN(due.getTime())) return null;
  const days = Math.round((due.getTime() - today.getTime()) / 86400000);
  if (days < 0) return "overdue";
  if (days <= DUE_SOON_DAYS) return "soon";
  return null;
}

function fmt_date(deadline: string): string {
  const d = new Date(`${deadline}T00:00:00`);
  return Number.isNaN(d.getTime()) ? deadline : d.toLocaleDateString();
}

export default function Kanban() {
  const nav = useNavigate();
  const { project_id } = use_projects();
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
  const [users, setUsers] = useState<UserInfo[]>([]);
  const [dComments, setDComments] = useState<Comment[]>([]);
  const [dCommentBody, setDCommentBody] = useState("");
  const [dTab, setDTab] = useState<"comments" | "history">("comments");
  const [dPriority, setDPriority] = useState<Priority>(PRIORITIES[1]);
  const [dDeadline, setDDeadline] = useState("");
  const [dLabel, setDLabel] = useState("");
  const [dCheckItem, setDCheckItem] = useState("");
  const [dEstimate, setDEstimate] = useState("");
  const [dThinking, setDThinking] = useState(false);
  const commentsEnd = useRef<HTMLDivElement | null>(null);
  const [dragOver, setDragOver] = useState<ColumnId | null>(null);
  const [view, setView] = useState<ViewMode>(VIEW_BOARD);
  const [showDone, setShowDone] = useState(false);
  const [draggingId, setDraggingId] = useState<number | null>(null);
  const [runningId, setRunningId] = useState<number | null>(null);
  const [cronjobs, setCronjobs] = useState<CronJob[]>([]);
  const [addOpen, setAddOpen] = useState(false);

  async function handle(err: unknown) {
    if (err instanceof Object && "status" in err && (err as { status?: number }).status === 401) {
      clear_token();
      nav("/");
      return;
    }
    setError(err instanceof Error ? err.message : String(err));
  }

  useEffect(() => {
    if (project_id == null) {
      setCards([]);
      return;
    }
    refresh();
  }, [project_id]);

  async function refresh() {
    try {
      setCards(await fetch_cards(project_id as number));
    } catch (err) {
      handle(err);
    }
  }

  async function add(e: React.FormEvent) {
    e.preventDefault();
    if (!title.trim() || project_id == null) return;
    setError("");
    try {
      await create_card(project_id, COLUMNS[0].id, title.trim(), "", priority);
      setTitle("");
      setAddOpen(false);
      await refresh();
      toast("task created", "success");
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
    setDPriority((card.priority as Priority) || PRIORITIES[1]);
    setDDeadline(card.deadline || "");
    setDLabel("");
    setDCheckItem("");
    setDEstimate(card.estimate != null ? String(card.estimate) : "");
    setDComments([]);
    setDCommentBody("");
    setDTab("comments");
    setDThinking(false);
    fetch_agents().then(setSavedAgents).catch(() => setSavedAgents([]));
    fetch_users().then(setUsers).catch(() => setUsers([]));
    fetch_cronjobs().then(setCronjobs).catch(() => setCronjobs([]));
    if (pipelines == null) {
      fetch_pipelines().then(setPipelines).catch(() => setPipelines([]));
    }
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
      await update_card(
        detail.id,
        dTitle.trim(),
        dDesc,
        dAssignee.trim() || null,
        dPriority,
        dDeadline
      );
      setDetail(null);
      await refresh();
    } catch (err) {
      handle(err);
    }
  }

  /// Names of agents that can be mentioned in comments.
  function mentionable_agents(): string[] {
    const names = savedAgents.map((a) => a.name).filter(Boolean);
    if (detail?.agent_name) names.push(detail.agent_name);
    return [...new Set(names)];
  }

  function mentioned_agent(body: string): string | null {
    const found = body.match(/@([\w.-]+)/g);
    if (!found) return null;
    const lower = found.map((m) => m.slice(1).toLowerCase());
    return mentionable_agents().find((n) => lower.includes(n.toLowerCase())) || null;
  }

  async function comment(e: React.FormEvent) {
    e.preventDefault();
    if (!detail || !dCommentBody.trim()) return;
    setError("");
    const body = dCommentBody.trim();
    const agent = mentioned_agent(body);
    try {
      await add_comment(detail.id, body);
      setDCommentBody("");
      setDComments(await fetch_comments(detail.id));
      if (agent) {
        setDThinking(true);
        try {
          const reply = await chat(`Card "${detail.title}": ${body.replace(new RegExp(`@${agent}`, "gi"), "").trim()}`);
          const text = String(reply.reply || "").trim();
          if (text) {
            await add_comment(detail.id, `${agent}: ${text}`);
          }
        } finally {
          setDThinking(false);
        }
      }
      setDComments(await fetch_comments(detail.id));
      commentsEnd.current?.scrollIntoView({ behavior: "smooth" });
    } catch (err) {
      setDThinking(false);
      handle(err);
    }
  }

  async function patch_detail(fields: {
    priority?: string;
    deadline?: string | null;
    pipeline_id?: number | null;
    agent_name?: string;
    agent_state?: unknown;
    cron?: string | null;
  }) {
    if (!detail) return null;
    setError("");
    try {
      if (fields.pipeline_id !== undefined) {
        await set_card_pipeline(detail.id, fields.pipeline_id);
      }
      if (fields.agent_name !== undefined) {
        await set_agent(detail.id, fields.agent_name, fields.agent_state ?? {});
      }
      if (fields.cron !== undefined) {
        await set_card_schedule(detail.id, fields.cron);
      }
      if (fields.priority !== undefined || fields.deadline !== undefined) {
        await update_card(
          detail.id,
          detail.title,
          detail.description,
          detail.assignee ?? null,
          fields.priority,
          fields.deadline,
          parse_labels(detail),
          parse_checklist(detail),
          detail.estimate
        );
      }
      await refresh();
      const updated = cards.find((c) => c.id === detail.id);
      return updated || null;
    } catch (err) {
      handle(err);
      return null;
    }
  }

  async function pick_detail_priority(p: Priority) {
    setDPriority(p);
    const updated = await patch_detail({ priority: p });
    if (updated) setDetail({ ...detail!, priority: p });
  }

  async function pick_detail_deadline(value: string) {
    setDDeadline(value);
    const updated = await patch_detail({ deadline: value });
    if (updated) setDetail({ ...detail!, deadline: value || null });
  }

  async function save_card_fields(
    id: number,
    patch: { labels?: string[]; checklist?: ChecklistItem[]; estimate?: number | null }
  ): Promise<Card | null> {
    const card = cards.find((c) => c.id === id);
    if (!card) return null;
    setError("");
    const labels = patch.labels ?? parse_labels(card);
    const checklist = patch.checklist ?? parse_checklist(card);
    const estimate = patch.estimate !== undefined ? patch.estimate : (card.estimate ?? null);
    try {
      const updated = await update_card(
        card.id,
        card.title,
        card.description,
        card.assignee ?? null,
        card.priority,
        card.deadline ?? null,
        labels,
        checklist,
        estimate
      );
      await refresh();
      if (detail?.id === id) {
        setDetail({
          ...detail,
          labels: JSON.stringify(labels),
          checklist: JSON.stringify(checklist),
          estimate,
        });
      }
      return updated;
    } catch (err) {
      handle(err);
      return null;
    }
  }

  function add_detail_label() {
    if (!detail) return;
    const names = dLabel
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    setDLabel("");
    if (names.length === 0) return;
    const next = [...new Set([...parse_labels(detail), ...names])];
    save_card_fields(detail.id, { labels: next });
  }

  function remove_detail_label(name: string) {
    if (!detail) return;
    save_card_fields(detail.id, { labels: parse_labels(detail).filter((l) => l !== name) });
  }

  function add_detail_check() {
    if (!detail || !dCheckItem.trim()) return;
    const next = [...parse_checklist(detail), { text: dCheckItem.trim(), done: false }];
    setDCheckItem("");
    save_card_fields(detail.id, { checklist: next });
  }

  function toggle_detail_check(index: number) {
    if (!detail) return;
    const items = parse_checklist(detail);
    if (!items[index]) return;
    save_card_fields(detail.id, {
      checklist: items.map((it, i) => (i === index ? { ...it, done: !it.done } : it)),
    });
  }

  function remove_detail_check(index: number) {
    if (!detail) return;
    save_card_fields(detail.id, {
      checklist: parse_checklist(detail).filter((_, i) => i !== index),
    });
  }

  function commit_detail_estimate() {
    if (!detail) return;
    const raw = dEstimate.trim();
    if (raw === "") {
      if (detail.estimate == null) return;
      save_card_fields(detail.id, { estimate: null });
      return;
    }
    const n = Number(raw);
    if (!Number.isInteger(n) || n < 0 || n > ESTIMATE_MAX) return;
    if (detail.estimate === n) return;
    save_card_fields(detail.id, { estimate: n });
  }

  async function pick_detail_pipeline(id: string) {
    const pipe_id = id === "" ? null : Number(id);
    const updated = await patch_detail({ pipeline_id: pipe_id });
    if (updated) setDetail({ ...detail!, pipeline_id: pipe_id });
  }

  async function pick_detail_schedule(expr: string) {
    if (expr !== "" && detail?.pipeline_id == null) {
      toast("attach a pipeline first", "error");
      return;
    }
    const updated = await patch_detail({ cron: expr || null });
    if (updated) setDetail({ ...detail!, cron: expr || null });
  }

  function pipe_name(card: Card): string {
    return (
      pipelines?.find((p) => p.id === card.pipeline_id)?.name ||
      card.pipeline_name ||
      `#${card.pipeline_id}`
    );
  }

  function card_chips(card: Card) {
    if (card.pipeline_id == null && !card.cron) return null;
    return (
      <span className="kanban-chips">
        {card.pipeline_id != null && (
          <button
            type="button"
            className="kanban-chip"
            onClick={() => open_pipe_modal(card)}
            title="pipeline — click to change"
          >
            <Workflow size={11} /> {pipe_name(card)}
          </button>
        )}
        {card.cron && (
          <button
            type="button"
            className="kanban-chip"
            onClick={() => open_detail(card)}
            title={`schedule — ${cron_label(card.cron)}`}
          >
            <Clock size={11} /> {cron_label(card.cron)}
          </button>
        )}
      </span>
    );
  }

  function pick_detail_bot(id: string) {
    const a = savedAgents.find((x) => x.id === Number(id));
    if (!a || !detail) return;
    set_agent(detail.id, a.name, { model: a.model, persona: a.persona, prompt: a.prompt, output: a.output })
      .then(async () => {
        await refresh();
        setDetail({ ...detail, agent_name: a.name });
      })
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

  async function run(card: Card) {
    setRunningId(card.id);
    setError("");
    try {
      await run_card(card.id);
      await refresh();
    } catch (err) {
      handle(err);
    } finally {
      setRunningId(null);
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

  const visibleColumns = COLUMNS.filter((c) => showDone || c.id !== "done");
  const listCards = [...cards].sort(
    (a, b) =>
      COLUMNS.findIndex((c) => c.id === a.column_id) -
        COLUMNS.findIndex((c) => c.id === b.column_id) || a.id - b.id
  );

  return (
    <main className="chat kanban-page">
      <header>
        <h1>kanban</h1>
        <span className="sub">
          {cards.length} task{cards.length === 1 ? "" : "s"}
        </span>
        <span className="kanban-view-toggle">
          <button
            onClick={() => setView(VIEW_BOARD)}
            title="board view"
            aria-pressed={view === VIEW_BOARD}
            className={view === VIEW_BOARD ? "active" : ""}
          >
            <LayoutGrid size={14} />
          </button>
          <button
            onClick={() => setView(VIEW_LIST)}
            title="list view"
            aria-pressed={view === VIEW_LIST}
            className={view === VIEW_LIST ? "active" : ""}
          >
            <List size={14} />
          </button>
          <button
            onClick={() => setShowDone((s) => !s)}
            title={showDone ? "hide done" : "show done"}
            aria-pressed={showDone}
          >
            {showDone ? <Eye size={14} /> : <EyeOff size={14} />}
          </button>
        </span>
      </header>
      <div className="kanban-add">
        <button
          onClick={() => setAddOpen(true)}
          disabled={project_id == null}
          title={project_id == null ? "create a workspace + project first…" : "add task"}
        >
          <Plus size={14} /> add task
        </button>
      </div>
      {view === VIEW_BOARD ? (
        <section
          className={visibleColumns.length === COLUMNS.length ? "kanban" : "kanban two-cols"}
        >
          {visibleColumns.map((col) => (
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
                    {card.pipeline_id != null && (
                      <button
                        onClick={() => run(card)}
                        title="run pipeline"
                        disabled={runningId != null}
                      >
                        {runningId === card.id ? <Loader2 size={13} className="spin" /> : <Play size={13} />}
                      </button>
                    )}
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
                <CardMeta card={card} />
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
                {card_chips(card)}
                <CardMeta card={card} />
                <RunBadge run={run_of(card)} on_open={() => open_detail(card)} />
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
      ) : (
        <section className="kanban-list">
          {listCards.map((card) => {
            const col = COLUMNS.find((c) => c.id === card.column_id);
            return (
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
                <span className={`kanban-prio prio-${card.priority}`}>{card.priority}</span>
                <span className="kanban-list-col">{col?.title}</span>
                <span className="kanban-title kanban-title-link" onClick={() => open_detail(card)} title="open card">
                  {card.title}
                </span>
                <CardMeta card={card} />
                {card_chips(card)}
                <RunBadge run={run_of(card)} on_open={() => open_detail(card)} />
                <span className="kanban-card-actions">
                  {card.pipeline_id != null && (
                    <button
                      onClick={() => run(card)}
                      title="run pipeline"
                      disabled={runningId != null}
                    >
                      {runningId === card.id ? <Loader2 size={13} className="spin" /> : <Play size={13} />}
                    </button>
                  )}
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
                <span className="kanban-move">
                  <button onClick={() => shift(card, -1)} disabled={card.column_id === COLUMNS[0].id}>
                    <ArrowLeft size={13} />
                  </button>
                  <button
                    onClick={() => shift(card, 1)}
                    disabled={card.column_id === COLUMNS[COLUMNS.length - 1].id}
                  >
                    <ArrowRight size={13} />
                  </button>
                </span>
              </div>
            );
          })}
        </section>
      )}
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
        <div className="kanban-detail">
          <section className="kanban-detail-left">
            <form className="kanban-detail-form" onSubmit={save_detail}>
              <input
                value={dTitle}
                onChange={(e) => setDTitle(e.target.value)}
                placeholder="title"
              />
              <textarea
                value={dDesc}
                onChange={(e) => setDDesc(e.target.value)}
                rows={6}
                spellCheck={false}
                placeholder="description…"
              />
              <div className="kanban-detail-actions">
                <button type="submit" disabled={!dTitle.trim()}><Save size={14} /> save</button>
              </div>
            </form>
            <div className="kanban-tabs" role="tablist">
              <button
                role="tab"
                aria-selected={dTab === "comments"}
                className={dTab === "comments" ? "active" : ""}
                onClick={() => setDTab("comments")}
              >
                comments
              </button>
              <button
                role="tab"
                aria-selected={dTab === "history"}
                className={dTab === "history" ? "active" : ""}
                onClick={() => setDTab("history")}
              >
                history
              </button>
            </div>
            {dTab === "comments" ? (
              <div className="kanban-comments">
                {dComments.map((c) => (
                  <div
                    key={c.id}
                    className={
                      mentionable_agents().some((n) => c.body.startsWith(`${n}: `))
                        ? "kanban-comment kanban-comment-agent"
                        : "kanban-comment"
                    }
                  >
                    <span className="kanban-comment-meta">
                      <strong>{c.author}</strong> {new Date(c.created_at).toLocaleString()}
                    </span>
                    <span>{c.body}</span>
                  </div>
                ))}
                {dThinking && (
                  <div className="kanban-comment kanban-comment-agent kanban-thinking">
                    <Loader2 size={13} className="spin" /> agent is thinking…
                  </div>
                )}
                <div ref={commentsEnd} />
                <form className="kanban-add" onSubmit={comment}>
                  <input
                    value={dCommentBody}
                    onChange={(e) => setDCommentBody(e.target.value)}
                    placeholder={`write a comment… (mention @${detail?.agent_name || "agent"} to chat)`}
                  />
                  <button type="submit" disabled={!dCommentBody.trim() || dThinking}>
                    <Plus size={14} />
                  </button>
                </form>
              </div>
            ) : (
              <RunTimeline run={detail ? run_of(cards.find((c) => c.id === detail.id) || detail) : null} />
            )}
          </section>
          <section className="kanban-detail-right">
            <div className="kanban-detail-field">
              <label htmlFor="kanban-detail-priority"><Flag size={13} /> priority</label>
              <select
                id="kanban-detail-priority"
                className="kanban-select"
                value={dPriority}
                onChange={(e) => pick_detail_priority(e.target.value as Priority)}
              >
                {PRIORITIES.map((p) => (
                  <option key={p} value={p}>{p}</option>
                ))}
              </select>
            </div>
            <div className="kanban-detail-field">
              <label htmlFor="kanban-detail-bot"><Bot size={13} /> agent</label>
              <select
                id="kanban-detail-bot"
                className="kanban-select"
                value=""
                onChange={(e) => pick_detail_bot(e.target.value)}
                title="assign bot (saved agent)"
              >
                <option value="">{detail?.agent_name || "assign bot…"}</option>
                {savedAgents.map((a) => (
                  <option key={a.id} value={a.id}>{a.name}</option>
                ))}
              </select>
            </div>
            <div className="kanban-detail-auto">
              <label htmlFor="kanban-detail-pipeline"><Workflow size={13} /> automation</label>
              <select
                id="kanban-detail-pipeline"
                className="kanban-select"
                value={detail?.pipeline_id != null ? String(detail.pipeline_id) : ""}
                onChange={(e) => pick_detail_pipeline(e.target.value)}
              >
                <option value="">no pipeline</option>
                {(pipelines || []).map((p) => (
                  <option key={p.id} value={p.id}>{p.name}</option>
                ))}
              </select>
              {detail?.pipeline_id != null && (
                <button
                  type="button"
                  className="kanban-run-inline"
                  onClick={() => run(detail)}
                  disabled={runningId != null}
                >
                  {runningId === detail.id ? <Loader2 size={13} className="spin" /> : <Play size={13} />} run now
                </button>
              )}
              <select
                id="kanban-detail-schedule"
                className="kanban-select"
                value={detail?.cron || ""}
                onChange={(e) => pick_detail_schedule(e.target.value)}
                disabled={detail?.pipeline_id == null}
                title="schedule pipeline runs (cron)"
              >
                <option value="">no schedule…</option>
                {CRON_PRESETS.map((p) => (
                  <option key={p.expr} value={p.expr}>{p.label}</option>
                ))}
                {detail?.cron && !CRON_PRESETS.some((p) => p.expr === detail.cron) && (
                  <option value={detail.cron}>{detail.cron}</option>
                )}
              </select>
              {detail?.pipeline_id == null ? (
                <span className="kanban-auto-hint">attach a pipeline to schedule runs</span>
              ) : (
                detail?.cron && (() => {
                  const next = next_run_for(detail.id, cronjobs);
                  return next != null ? (
                    <span className="kanban-auto-hint">
                      <Clock size={11} /> next run {fmt_time(next)}
                    </span>
                  ) : null;
                })()
              )}
            </div>
            <div className="kanban-detail-field">
              <label htmlFor="kanban-detail-person"><User size={13} /> assignee</label>
              <input
                id="kanban-detail-person"
                className="kanban-select"
                value={dAssignee}
                onChange={(e) => setDAssignee(e.target.value)}
                placeholder="assign person…"
                list="kanban-user-list"
              />
              <datalist id="kanban-user-list">
                {users.map((u) => (
                  <option key={u.username} value={u.username} />
                ))}
              </datalist>
            </div>
            <div className="kanban-detail-field">
              <label htmlFor="kanban-detail-deadline"><CalendarClock size={13} /> deadline</label>
              <input
                id="kanban-detail-deadline"
                type="date"
                className="kanban-select"
                value={dDeadline}
                onChange={(e) => pick_detail_deadline(e.target.value)}
              />
            </div>
            <div className="kanban-detail-field">
              <label htmlFor="kanban-detail-label"><Tag size={13} /> labels</label>
              {detail && parse_labels(detail).length > 0 && (
                <div className="kanban-label-edit">
                  {parse_labels(detail).map((l) => (
                    <span key={l} className="kanban-label" style={{ borderColor: label_color(l) }}>
                      <span className="kanban-label-dot" style={{ background: label_color(l) }} />
                      {l}
                      <button
                        type="button"
                        onClick={() => remove_detail_label(l)}
                        title={`remove ${l}`}
                        aria-label={`remove label ${l}`}
                      >
                        <X size={10} />
                      </button>
                    </span>
                  ))}
                </div>
              )}
              <input
                id="kanban-detail-label"
                className="kanban-select"
                value={dLabel}
                onChange={(e) => setDLabel(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    add_detail_label();
                  }
                }}
                placeholder="add label… (comma splits)"
              />
            </div>
            <div className="kanban-detail-field">
              <label htmlFor="kanban-detail-check"><ListChecks size={13} /> checklist</label>
              {detail && parse_checklist(detail).length > 0 && (
                <div className="kanban-check-list">
                  {parse_checklist(detail).map((item, i) => (
                    <span key={i} className="kanban-check-item">
                      <input
                        type="checkbox"
                        checked={item.done}
                        onChange={() => toggle_detail_check(i)}
                        aria-label={`toggle ${item.text}`}
                      />
                      <span className={item.done ? "kanban-check-text done" : "kanban-check-text"}>
                        {item.text}
                      </span>
                      <button
                        type="button"
                        onClick={() => remove_detail_check(i)}
                        title="remove item"
                        aria-label={`remove checklist item ${item.text}`}
                      >
                        <X size={10} />
                      </button>
                    </span>
                  ))}
                </div>
              )}
              <input
                id="kanban-detail-check"
                className="kanban-select"
                value={dCheckItem}
                onChange={(e) => setDCheckItem(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    add_detail_check();
                  }
                }}
                placeholder="add checklist item…"
              />
            </div>
            <div className="kanban-detail-field">
              <label htmlFor="kanban-detail-estimate"><Hash size={13} /> estimate</label>
              <input
                id="kanban-detail-estimate"
                type="number"
                min={0}
                max={ESTIMATE_MAX}
                className="kanban-select"
                value={dEstimate}
                onChange={(e) => setDEstimate(e.target.value)}
                onBlur={commit_detail_estimate}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    commit_detail_estimate();
                  }
                }}
                placeholder="points (0–99)"
              />
            </div>
          </section>
        </div>
      </SlideOver>
      <Modal open={addOpen} title="new task" on_close={() => setAddOpen(false)}>
        <form className="modal-form" onSubmit={add}>
          <input
            autoFocus
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="task title…"
            required
          />
          <select value={priority} onChange={(e) => setPriority(e.target.value as Priority)}>
            {PRIORITIES.map((p) => (
              <option key={p} value={p}>{p}</option>
            ))}
          </select>
          <button type="submit">add</button>
        </form>
      </Modal>
    </main>
  );
}

const STAGE_LABEL: Record<string, string> = {
  ok: "passed",
  failed: "failed",
};

function RunBadge({ run, on_open }: { run: CardRun | null; on_open: () => void }) {
  if (!run) return null;
  return (
    <button
      className={`run-badge run-${run.status}`}
      onClick={on_open}
      title="open run log"
    >
      {run.status === "ok" ? <CheckCircle2 size={11} /> : <XCircle size={11} />}
      {run.pipeline_name} · {run.status === "ok" ? "ran" : "failed"} · {run.stages.length} stage{run.stages.length === 1 ? "" : "s"}
    </button>
  );
}

function CardMeta({ card }: { card: Card }) {
  const labels = parse_labels(card);
  const checklist = parse_checklist(card);
  const done = checklist.filter((c) => c.done).length;
  const due = due_state(card.deadline);
  const pct = checklist.length > 0 ? Math.round((done / checklist.length) * 100) : 0;
  const has_meta =
    labels.length > 0 || checklist.length > 0 || card.deadline != null || card.estimate != null;
  if (!has_meta) return null;
  return (
    <div className="kanban-card-meta">
      {labels.map((l) => (
        <span key={l} className="kanban-label" style={{ borderColor: label_color(l) }}>
          <span className="kanban-label-dot" style={{ background: label_color(l) }} />
          {l}
        </span>
      ))}
      {checklist.length > 0 && (
        <span className="kanban-check-progress" title={`checklist ${done}/${checklist.length}`}>
          <span className="kanban-check-bar">
            <span className="kanban-check-bar-fill" style={{ width: `${pct}%` }} />
          </span>
          {done}/{checklist.length}
        </span>
      )}
      {card.deadline && (
        <span className={due ? `kanban-due ${due}` : "kanban-due"} title="deadline">
          <CalendarClock size={11} /> {fmt_date(card.deadline)}
        </span>
      )}
      {card.estimate != null && (
        <span className="kanban-estimate" title="estimate">
          <Zap size={11} /> {card.estimate}
        </span>
      )}
    </div>
  );
}

function RunTimeline({ run }: { run: CardRun | null }) {
  if (!run) return null;
  return (
    <section className="run-timeline" aria-label="pipeline run log">
      <h3>
        <Workflow size={13} /> run · {run.pipeline_name} ·{" "}
        <span className={run.status === "ok" ? "run-ok-text" : "run-failed-text"}>{STAGE_LABEL[run.status]}</span>
      </h3>
      <ol>
        {run.stages.map((s, i) => (
          <li key={`${s.node}-${i}`} className={`run-stage run-${s.status}`}>
            <span className="run-stage-icon">
              {s.status === "ok" ? <CheckCircle2 size={13} /> : <XCircle size={13} />}
            </span>
            <span className="run-stage-body">
              <strong>{s.node}</strong>
              <span className="run-stage-stage">{s.stage}</span>
              <span className="run-stage-note">{s.note}</span>
            </span>
          </li>
        ))}
      </ol>
      {run.output && <pre className="run-output">{run.output}</pre>}
    </section>
  );
}
