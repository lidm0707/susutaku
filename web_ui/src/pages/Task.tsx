import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { ArrowLeft, ArrowRight, Bot, CalendarClock, CheckCircle2, Eye, EyeOff, Flag, Hash, Image, LayoutGrid, List, ListChecks, Loader2, Play, Plus, Save, Tag, Trash2, Upload, User, X, XCircle, Zap } from "lucide-react";
import {
  clear_token,
  connect_events,
  add_comment,
  cancel_card_run,
  cancel_chat_run,
  chat_zai_stream,
  create_task,
  fetch_agents,
  fetch_card_events,
  fetch_tasks,
  fetch_comments,
  fetch_projects,
  fetch_users,
  fetch_agent_machine,
  query_param,
  set_query_param,
  PARAM_CARD,
  PARAM_PROJECT,
  remove_card,
  set_agent,
  set_card_image,
  set_card_schedule,
  run_card,
  run_of,
  progress_of,
  fetch_active_runs,
  finish_manager_agent,
  set_task_status,
  update_card,
  type Agent,
  type Card,
  type CardRun,
  type ChecklistItem,
  type Comment,
  type UserInfo,
  type StoredOutcome,
  type ActiveRun,
  type ChatStreamEvent,
  type RunEvent,
} from "../lib.js";
import { Modal, SlideOver } from "../ui/Overlay.js";
import { toast } from "../ui/Toast.js";
import { use_projects } from "../components/ProjectContext.tsx";
import { emit_card_created } from "../features/card_bus.js";
import { use_workspaces } from "../components/WorkspaceContext.tsx";

const COLUMNS = [
  { id: "todo", title: "To Do" },
  { id: "doing", title: "In Progress" },
  { id: "review", title: "Review" },
  { id: "conflict", title: "Conflict" },
  { id: "done", title: "Done" },
  { id: "failed", title: "Failed" },
] as const;

const PRIORITIES = ["low", "normal", "high", "critical"] as const;

type ColumnId = (typeof COLUMNS)[number]["id"];
type Priority = (typeof PRIORITIES)[number];

const VIEW_BOARD = "board";
const CARD_MIME = "application/x-susutaku-card";
const VIEW_LIST = "list";
type ViewMode = typeof VIEW_BOARD | typeof VIEW_LIST;

const LABEL_PALETTE = ["#7bd88f", "#e3b341", "#ff7b7b", "#6fb3ff", "#c792ea", "#64d8cb"];
const DUE_SOON_DAYS = 2;
const ESTIMATE_MAX = 99;
/// Trailing tool-trace lines kept when an agent comment is posted.
const TOOL_TRACE_MAX = 10;
/// Manager slot key separator: a card run's slot is `<agent>#<card id>`.
const SLOT_SEP = "#";
const PUSH_FAILED_PREFIX = "push failed";

type DueState = "overdue" | "soon" | null;

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

export default function Task() {
  const nav = useNavigate();
  const { project_id, pick_project } = use_projects();
  const { workspaces, pick: pick_ws } = use_workspaces();
  const [cards, setCards] = useState<Card[]>([]);
  const [, setError] = useState("");
  const [title, setTitle] = useState("");
  const [desc, setDesc] = useState("");
  const [priority, setPriority] = useState<Priority>(PRIORITIES[1]);
  const [agentFor, setAgentFor] = useState<Card | null>(null);
  const [agentName, setAgentName] = useState("");
  const [agentState, setAgentState] = useState("{}");
  const [agentMachine, setAgentMachine] = useState<string | null>(null);
  const [savedAgents, setSavedAgents] = useState<Agent[]>([]);
  const [imgFor, setImgFor] = useState<Card | null>(null);
  const [imgPick, setImgPick] = useState("");
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
  const [dImage, setDImage] = useState("");
  const [dThinking, setDThinking] = useState(false);
  const [dAgentLive, setDAgentLive] = useState("");
  const mentionRunId = useRef<string | null>(null);
  const [mentionQuery, setMentionQuery] = useState<string | null>(null);
  const commentsEnd = useRef<HTMLDivElement | null>(null);
  const [dragOver, setDragOver] = useState<ColumnId | null>(null);
  const [view, setView] = useState<ViewMode>(VIEW_LIST);
  const [showDone, setShowDone] = useState(false);
  const [draggingId, setDraggingId] = useState<number | null>(null);
  const [runningId, setRunningId] = useState<number | null>(null);
  const [finishingId, setFinishingId] = useState<number | null>(null);
  const [activeRuns, setActiveRuns] = useState<ActiveRun[]>([]);
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
    fetch_active_runs().then(setActiveRuns).catch(() => setActiveRuns([]));
  }, [project_id]);

  useEffect(() => {
    if (project_id == null) return;
    return connect_events((e) => {
      if (e.kind !== "card") return;
      if (document.visibilityState !== "visible") return;
      refresh();
      fetch_active_runs().then(setActiveRuns).catch(() => {});
    });
  }, [project_id]);

  // Deep link ?project=<id>&card=<id>: switch to the card's project, then let
  // the cards effect open the detail. The detail effect must not wipe the URL
  // params before the cards load, hence the first-run skips below.
  const deep_project = useRef(query_param(PARAM_PROJECT));
  const deep_done = useRef(deep_project.current == null);
  const detail_touched = useRef(false);

  useEffect(() => {
    if (deep_done.current || workspaces.length === 0) return;
    const want = Number(deep_project.current);
    (async () => {
      for (const ws of workspaces) {
        const projects = await fetch_projects(ws.id).catch(() => []);
        if (projects.some((p) => p.id === want)) {
          pick_ws(ws.id);
          pick_project(want);
          break;
        }
      }
      deep_done.current = true;
    })();
  }, [workspaces]);

  useEffect(() => {
    if (!detail_touched.current) {
      detail_touched.current = true;
      return;
    }
    set_query_param(PARAM_CARD, detail ? String(detail.id) : null);
  }, [detail]);

  useEffect(() => {
    if (cards.length === 0 || detail != null) return;
    const id = query_param(PARAM_CARD);
    if (id == null) return;
    const card = cards.find((c) => String(c.id) === id);
    if (card) open_detail(card);
    else if (deep_done.current) set_query_param(PARAM_CARD, null);
  }, [cards]);

  async function refresh() {
    try {
      setCards(await fetch_tasks(project_id as number));
    } catch (err) {
      handle(err);
    }
  }

  async function add(e: React.FormEvent) {
    e.preventDefault();
    if (!title.trim() || project_id == null) return;
    setError("");
    try {
      const res = await create_task(project_id ?? null, title.trim(), desc.trim(), priority);
      if (res.ok) {
        const card = (await res.json()) as { id: number; title: string; project_id: number | null };
        emit_card_created({ id: card.id, title: card.title, project_id: card.project_id });
      }
      setTitle("");
      setDesc("");
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
      await set_task_status(card.id, next);
      await refresh();
    } catch (err) {
      handle(err);
    }
  }

  async function drop(card: Card, column_id: ColumnId) {
    if (card.column_id === column_id) return;
    setError("");
    try {
      await set_task_status(card.id, column_id);
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
    setAgentMachine(null);
    if (card.agent_name) {
      fetch_agent_machine(card.agent_name)
        .then((w) => setAgentMachine(w.machine))
        .catch(() => setAgentMachine(""));
    }
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
    setDImage(card.image || "");
    setDComments([]);
    setDCommentBody("");
    setDTab("comments");
    setDThinking(false);
    fetch_agents().then(setSavedAgents).catch(() => setSavedAgents([]));
    fetch_users().then(setUsers).catch(() => setUsers([]));
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

  /// Active @mention query derived from the comment body, or null when not mentioning.
  const mention_matches =
    mentionQuery == null
      ? []
      : mentionable_agents().filter((n) =>
          n.toLowerCase().startsWith(mentionQuery.toLowerCase())
        );

  function pick_mention(name: string) {
    setDCommentBody((b) => b.replace(/@([\w.-]*)$/, `@${name} `));
    setMentionQuery(null);
  }

  function mentioned_agent(body: string): string | null {
    const found = body.match(/@[\w.-]+/g);
    if (!found) return null;
    const lower = found.map((m) => m.slice(1).toLowerCase());
    return mentionable_agents().find((n) => lower.includes(n.toLowerCase())) || null;
  }

  function make_run_id(): string {
    return typeof crypto.randomUUID === "function"
      ? crypto.randomUUID()
      : `run-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  }

  /// Stream error messages that mean "stopped on purpose" — the partial
  /// output is kept and posted instead of a failed bubble.
  const INTERRUPT_MSGS = ["interrupted", "stream ended without a done event"];

  /// Trailing tool-trace lines appended to an agent comment so the full run
  /// output (not just the prose reply) lands on the card.
  function trace_note(tools: string[]): string {
    if (!tools.length) return "";
    const tail = tools.slice(-TOOL_TRACE_MAX).join("\n");
    return `\n\ntool trace:\n${tail}`;
  }

  function stop_mention() {
    if (mentionRunId.current) cancel_chat_run(mentionRunId.current);
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
        setDAgentLive("");
        const run_id = make_run_id();
        mentionRunId.current = run_id;
        const tools: string[] = [];
        let streamed = "";
        try {
          const text = body.replace(new RegExp(`@${agent}`, "gi"), "").trim();
          const reply = await chat_zai_stream(
            text,
            "",
            undefined,
            agent,
            undefined,
            undefined,
            (ev: ChatStreamEvent) => {
              if (ev.type === "turn") {
                streamed = "";
                setDAgentLive("");
              } else if (ev.type === "delta") {
                streamed += ev.text;
                setDAgentLive(streamed);
              } else if (ev.type === "tool") {
                tools.push(`${ev.tool}: ${ev.summary || ev.input}`);
              }
            },
            run_id
          );
          const out = String(reply.reply || streamed).trim();
          if (out || tools.length) {
            await add_comment(detail.id, `${agent}: ${out}${trace_note(tools)}`);
          }
        } catch (err) {
          // an interrupted or dropped stream keeps its partial output
          const msg = String((err as Error)?.message ?? err);
          const partial = streamed.trim();
          if (INTERRUPT_MSGS.includes(msg) && (partial || tools.length)) {
            await add_comment(
              detail.id,
              `${agent}: (interrupted)${partial ? `\n${partial}` : ""}${trace_note(tools)}`
            );
          } else {
            throw err;
          }
        } finally {
          mentionRunId.current = null;
          setDThinking(false);
          setDAgentLive("");
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
    image?: string | null;
    agent_name?: string;
    agent_state?: unknown;
    cron?: string | null;
  }) {
    if (!detail) return null;
    setError("");
    try {
      if (fields.image !== undefined) {
        await set_card_image(detail.id, fields.image);
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

  async function pick_detail_image(value: string) {
    setDImage(value);
    const updated = await patch_detail({ image: value.trim() || null });
    if (updated) setDetail({ ...detail!, image: value.trim() || null });
  }

  function card_chips(card: Card) {
    if (!card.agent_name && card.image == null) return null;
    return (
      <span className="task-chips">
        {card.agent_name && (
          <button
            type="button"
            className="task-chip"
            onClick={() => open_agent_modal(card)}
            title="agent — click to change"
          >
            <Bot size={11} /> {card.agent_name}
          </button>
        )}
        {card.image != null && (
          <button
            type="button"
            className="task-chip"
            onClick={() => open_img_modal(card)}
            title="image — click to change"
          >
            <Image size={11} /> {card.image}
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

  function open_img_modal(card: Card) {
    setImgFor(card);
    setImgPick(card.image || "");
  }

  async function save_image(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    try {
      await set_card_image((imgFor as Card).id, imgPick.trim() || null);
      setImgFor(null);
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

  /// Finish the card's agent task slot (`<agent>#<card id>`) and push its
  /// branch to the project's bound repo.
  async function finish_push(card: Card) {
    if (!card.agent_name) return;
    setFinishingId(card.id);
    try {
      const slot = `${card.agent_name}${SLOT_SEP}${card.id}`;
      const outcome: StoredOutcome = await finish_manager_agent(slot, {
        push: true,
        project_id: card.project_id ?? undefined,
      });
      const pushed = outcome.push
        ? ` — ${outcome.push.startsWith(PUSH_FAILED_PREFIX) ? outcome.push : "pushed"}`
        : "";
      toast(`card ${card.id} finished — output on card${pushed}`);
      await refresh();
    } catch (err) {
      toast(err instanceof Error ? err.message : String(err));
    } finally {
      setFinishingId(null);
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
    <main className="chat task-page">
      <header>
        <h1>task</h1>
        <span className="sub">
          {cards.length} task{cards.length === 1 ? "" : "s"}
        </span>
        <span className="task-view-toggle">
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
      <ActiveStrip
        runs={activeRuns}
        on_open={(card) => {
          if (card.project_id != null && card.project_id !== project_id) {
            nav(`?project=${card.project_id}&card=${card.id}`);
            return;
          }
          const found = cards.find((c) => c.id === card.id);
          if (found) open_detail(found);
          else nav(`?project=${card.project_id ?? ""}&card=${card.id}`);
        }}
      />
      <div className="task-add">
        <button
          onClick={() => setAddOpen(true)}
          disabled={project_id == null}
          title={project_id == null ? "create a workspace + project first…" : "add task"}
        >
          <Plus size={14} /> add task
        </button>
      </div>
      {view === VIEW_BOARD ? (
        <section className="task">
          {visibleColumns.map((col) => (
          <div
            key={col.id}
            className={dragOver === col.id ? "task-col drag-over" : "task-col"}
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
                className={draggingId === card.id ? "task-card dragging" : "task-card"}
                draggable
                onDragStart={(e: React.DragEvent<HTMLDivElement>) => {
                  e.dataTransfer.setData("text/plain", String(card.id));
                  e.dataTransfer.setData(CARD_MIME, String(card.id));
                  setDraggingId(card.id);
                }}
                onDragEnd={() => setDraggingId(null)}
              >
                <div className="task-card-top">
                  <span className={`task-prio prio-${card.priority}`}>{card.priority}</span>
                  <span className="task-card-actions">
                    <button
                      onClick={() => run(card)}
                      title="run agent"
                      disabled={runningId != null}
                    >
                      {runningId === card.id ? <Loader2 size={13} className="spin" /> : <Play size={13} />}
                    </button>
                    {card.agent_name && (
                      <button
                        onClick={() => finish_push(card)}
                        title="finish + push the task branch"
                        disabled={finishingId != null}
                      >
                        {finishingId === card.id ? <Loader2 size={13} className="spin" /> : <Upload size={13} />}
                      </button>
                    )}
                    <button
                      onClick={() => open_img_modal(card)}
                      title="set image"
                      className={card.image != null ? "agent-set" : ""}
                    >
                      <Image size={13} />
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
                <span className="task-title task-title-link" onClick={() => open_detail(card)} title="open card">
                  {card.title}
                </span>
                <CardMeta card={card} />
                {card.description && (
                  <span className="task-desc" title={card.description}>
                    {card.description}
                  </span>
                )}
                {card.agent_name && (
                  <span className="task-agent">
                    <Bot size={11} /> {card.agent_name}
                  </span>
                )}
                {card.assignee && (
                  <span className="task-agent">
                    <User size={11} /> {card.assignee}
                  </span>
                )}
                {card_chips(card)}
                <CardMeta card={card} />
                <RunBadge run={run_of(card)} card={card} on_open={() => open_detail(card)} />
                <span className="task-move">
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
        <section className="task-list">
          {listCards.map((card) => {
            const col = COLUMNS.find((c) => c.id === card.column_id);
            return (
              <div
                key={card.id}
                className={draggingId === card.id ? "task-card dragging" : "task-card"}
                draggable
                onDragStart={(e: React.DragEvent<HTMLDivElement>) => {
                  e.dataTransfer.setData("text/plain", String(card.id));
                  e.dataTransfer.setData(CARD_MIME, String(card.id));
                  setDraggingId(card.id);
                }}
                onDragEnd={() => setDraggingId(null)}
              >
                <span className={`task-prio prio-${card.priority}`}>{card.priority}</span>
                <span className="task-list-col">{col?.title}</span>
                <span className="task-title task-title-link" onClick={() => open_detail(card)} title="open card">
                  {card.title}
                </span>
                <CardMeta card={card} />
                {card_chips(card)}
                <RunBadge run={run_of(card)} card={card} on_open={() => open_detail(card)} />
                <span className="task-card-actions">
                  <button
                    onClick={() => run(card)}
                    title="run agent"
                    disabled={runningId != null}
                  >
                    {runningId === card.id ? <Loader2 size={13} className="spin" /> : <Play size={13} />}
                  </button>
                  {card.agent_name && (
                    <button
                      onClick={() => finish_push(card)}
                      title="finish + push the task branch"
                      disabled={finishingId != null}
                    >
                      {finishingId === card.id ? <Loader2 size={13} className="spin" /> : <Upload size={13} />}
                    </button>
                  )}
                  <button
                    onClick={() => open_img_modal(card)}
                    title="set image"
                    className={card.image != null ? "agent-set" : ""}
                  >
                    <Image size={13} />
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
                <span className="task-move">
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
              className="task-select"
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
            {agentName.trim() && (
              <p className="modal-hint">
                {agentMachine == null
                  ? "checking where this agent runs…"
                  : agentMachine
                    ? `runs on ${agentMachine}`
                    : "not running on any machine right now"}
              </p>
            )}
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
        open={imgFor != null}
        title={<><Image size={14} /> image on card #{imgFor?.id}</>}
        on_close={() => setImgFor(null)}
      >
        <form className="modal-form" onSubmit={save_image}>
            <input
              className="task-select"
              value={imgPick}
              onChange={(e) => setImgPick(e.target.value)}
              placeholder="image name… (empty clears)"
            />
            <button type="submit">save</button>
        </form>
      </Modal>
      <SlideOver
        open={detail != null}
        title={<>card #{detail?.id}</>}
        on_close={() => {
          setDetail(null);
        }}
      >
        <div className="task-detail">
          <section className="task-detail-left">
            <form className="task-detail-form" onSubmit={save_detail}>
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
              <div className="task-detail-actions">
                <button type="submit" disabled={!dTitle.trim()}><Save size={14} /> save</button>
              </div>
            </form>
            <div className="task-tabs" role="tablist">
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
              <div className="task-comments">
                {dComments.map((c) => (
                  <div
                    key={c.id}
                    className={
                      mentionable_agents().some((n) => c.body.startsWith(`${n}: `))
                        ? "task-comment task-comment-agent"
                        : "task-comment"
                    }
                  >
                    <span className="task-comment-meta">
                      <strong>{c.author}</strong> {new Date(c.created_at).toLocaleString()}
                    </span>
                    <span>{c.body}</span>
                  </div>
                ))}
                {dThinking && (
                  <div className="task-comment task-comment-agent task-thinking" aria-live="polite">
                    <div>
                      <Loader2 size={13} className="spin" /> agent is thinking…
                      <button
                        type="button"
                        className="task-mention-stop"
                        onClick={stop_mention}
                        title="interrupt this agent run"
                      >
                        <XCircle size={13} /> stop
                      </button>
                    </div>
                    {dAgentLive && <pre className="task-agent-live">{dAgentLive}</pre>}
                  </div>
                )}
                <div ref={commentsEnd} />
                <form className="task-add" onSubmit={comment}>
                  {mention_matches.length > 0 && (
                    <ul className="task-mention-list" role="listbox">
                      {mention_matches.map((n) => (
                        <li key={n} role="option" aria-selected={false}>
                          <button type="button" onClick={() => pick_mention(n)}>
                            @{n}
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                  <input
                    value={dCommentBody}
                    onChange={(e) => {
                      const v = e.target.value;
                      setDCommentBody(v);
                      const m = v.match(/@([\w.-]*)$/);
                      setMentionQuery(m ? m[1] : null);
                    }}
                    placeholder={`write a comment… (mention @${detail?.agent_name || "agent"} to chat)`}
                  />
                  <button type="submit" disabled={!dCommentBody.trim() || dThinking}>
                    <Plus size={14} />
                  </button>
                </form>
              </div>
            ) : (
              <RunLive
                card={detail ? cards.find((c) => c.id === detail.id) ?? detail : null}
                run={detail ? run_of(cards.find((c) => c.id === detail.id) || detail) : null}
              />
            )}
          </section>
          <section className="task-detail-right">
            <div className="task-detail-field">
              <label htmlFor="task-detail-priority"><Flag size={13} /> priority</label>
              <select
                id="task-detail-priority"
                className="task-select"
                value={dPriority}
                onChange={(e) => pick_detail_priority(e.target.value as Priority)}
              >
                {PRIORITIES.map((p) => (
                  <option key={p} value={p}>{p}</option>
                ))}
              </select>
            </div>
            <div className="task-detail-field">
              <label htmlFor="task-detail-bot"><Bot size={13} /> agent</label>
              <select
                id="task-detail-bot"
                className="task-select"
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
            <div className="task-detail-auto">
              <label htmlFor="task-detail-image"><Image size={13} /> image</label>
              <input
                id="task-detail-image"
                className="task-select"
                value={dImage}
                onChange={(e) => setDImage(e.target.value)}
                onBlur={(e) => {
                  if (e.target.value !== (detail?.image || "")) pick_detail_image(e.target.value);
                }}
                placeholder="image name… (empty clears)"
              />
              {detail && (
                <>
                  <button
                    type="button"
                    className="task-run-inline"
                    onClick={() => run(detail)}
                    disabled={runningId != null}
                  >
                    {runningId === detail.id ? <Loader2 size={13} className="spin" /> : <Play size={13} />} run now
                  </button>
                  <button
                    type="button"
                    className="task-run-inline"
                    onClick={async () => {
                      await cancel_card_run(detail.id).catch(() => {});
                      await refresh();
                    }}
                    title="interrupt the in-flight run at the next tool round"
                  >
                    <XCircle size={13} /> stop run
                  </button>
                </>
              )}
              {detail?.agent_name && (
                <button
                  type="button"
                  className="task-run-inline"
                  onClick={() => finish_push(detail)}
                  disabled={finishingId != null}
                >
                  {finishingId === detail.id ? (
                    <Loader2 size={13} className="spin" />
                  ) : (
                    <Upload size={13} />
                  )}{' '}finish + push
                </button>
              )}
            </div>
            <div className="task-detail-field">
              <label htmlFor="task-detail-person"><User size={13} /> assignee</label>
              <input
                id="task-detail-person"
                className="task-select"
                value={dAssignee}
                onChange={(e) => setDAssignee(e.target.value)}
                placeholder="assign person…"
                list="task-user-list"
              />
              <datalist id="task-user-list">
                {users.map((u) => (
                  <option key={u.username} value={u.username} />
                ))}
              </datalist>
            </div>
            <div className="task-detail-field">
              <label htmlFor="task-detail-deadline"><CalendarClock size={13} /> deadline</label>
              <input
                id="task-detail-deadline"
                type="date"
                className="task-select"
                value={dDeadline}
                onChange={(e) => pick_detail_deadline(e.target.value)}
              />
            </div>
            <div className="task-detail-field">
              <label htmlFor="task-detail-label"><Tag size={13} /> labels</label>
              {detail && parse_labels(detail).length > 0 && (
                <div className="task-label-edit">
                  {parse_labels(detail).map((l) => (
                    <span key={l} className="task-label" style={{ borderColor: label_color(l) }}>
                      <span className="task-label-dot" style={{ background: label_color(l) }} />
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
                id="task-detail-label"
                className="task-select"
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
            <div className="task-detail-field">
              <label htmlFor="task-detail-check"><ListChecks size={13} /> checklist</label>
              {detail && parse_checklist(detail).length > 0 && (
                <div className="task-check-list">
                  {parse_checklist(detail).map((item, i) => (
                    <span key={i} className="task-check-item">
                      <input
                        type="checkbox"
                        checked={item.done}
                        onChange={() => toggle_detail_check(i)}
                        aria-label={`toggle ${item.text}`}
                      />
                      <span className={item.done ? "task-check-text done" : "task-check-text"}>
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
                id="task-detail-check"
                className="task-select"
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
            <div className="task-detail-field">
              <label htmlFor="task-detail-estimate"><Hash size={13} /> estimate</label>
              <input
                id="task-detail-estimate"
                type="number"
                min={0}
                max={ESTIMATE_MAX}
                className="task-select"
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
      <Modal open={addOpen} title="Create Task" on_close={() => setAddOpen(false)}>
        <form className="modal-form" onSubmit={add}>
          <label htmlFor="task-add-title">Title</label>
          <input
            id="task-add-title"
            autoFocus
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="task title…"
            required
          />
          <label htmlFor="task-add-desc">Description</label>
          <textarea
            id="task-add-desc"
            value={desc}
            onChange={(e) => setDesc(e.target.value)}
            placeholder="what needs doing?"
            rows={3}
          />
          <label htmlFor="task-add-priority">Priority</label>
          <select id="task-add-priority" value={priority} onChange={(e) => setPriority(e.target.value as Priority)}>
            {PRIORITIES.map((p) => (
              <option key={p} value={p}>{p}</option>
            ))}
          </select>
          <button type="submit">Create Task</button>
        </form>
      </Modal>
    </main>
  );
}

const STAGE_LABEL: Record<string, string> = {
  ok: "passed",
  failed: "failed",
};

function RunBadge({
  run,
  card,
  on_open,
}: {
  run: CardRun | null;
  card: Card;
  on_open: () => void;
}) {
  if (!run) {
    // Live/failed state straight off the run record fields (no run log yet).
    if (card.run_status === "running" || card.run_status === "error") {
      return (
        <button className={`run-badge run-${card.run_status === "running" ? "ok" : "failed"}`} onClick={on_open}>
          {card.run_status === "running" ? <Loader2 size={11} className="spin" /> : <XCircle size={11} />}
          {card.run_status === "running" ? `running${card.last_agent ? ` · ${card.last_agent}` : ""}` : "run error"}
        </button>
      );
    }
    return null;
  }
  const failed_note = run.stages?.find((s) => s.status === "failed")?.note ?? "";
  return (
    <button
      className={`run-badge run-${run.status}`}
      onClick={on_open}
      title={failed_note || "open run log"}
    >
      {run.status === "ok" ? <CheckCircle2 size={11} /> : <XCircle size={11} />}
      {card.last_agent ? `${card.last_agent} · ` : ""}{run.status === "ok" ? "ran" : `failed · ${failed_note}`}
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
    <div className="task-card-meta">
      {labels.map((l) => (
        <span key={l} className="task-label" style={{ borderColor: label_color(l) }}>
          <span className="task-label-dot" style={{ background: label_color(l) }} />
          {l}
        </span>
      ))}
      {checklist.length > 0 && (
        <span className="task-check-progress" title={`checklist ${done}/${checklist.length}`}>
          <span className="task-check-bar">
            <span className="task-check-bar-fill" style={{ width: `${pct}%` }} />
          </span>
          {done}/{checklist.length}
        </span>
      )}
      {card.deadline && (
        <span className={due ? `task-due ${due}` : "task-due"} title="deadline">
          <CalendarClock size={11} /> {fmt_date(card.deadline)}
        </span>
      )}
      {card.estimate != null && (
        <span className="task-estimate" title="estimate">
          <Zap size={11} /> {card.estimate}
        </span>
      )}
    </div>
  );
}

// Live run view inside the card: the agent stream (generation, tool calls,
// command output) is persisted in the DB per card, so this replays it via
// fetch + incremental `after` polling and nothing is lost on reconnect; once
// a record exists the finished run timeline rides above the stream.
const RUN_EVENT_POLL_MS = 1500;
const RUN_EVENT_LABEL: Record<string, string> = {
  run: "run",
  gen: "agent",
  tool: "tool",
  out: "output",
  retry: "retry",
  note: "note",
  final: "result",
};

function RunLive({ card, run }: { card: Card | null; run: CardRun | null }) {
  const live = card?.run_status === "running";
  const card_id = card?.id ?? null;
  const [events, setEvents] = useState<RunEvent[]>([]);
  useEffect(() => {
    if (card_id == null) {
      setEvents([]);
      return;
    }
    let alive = true;
    let after = 0;
    setEvents([]);
    const load = async () => {
      try {
        const rows = await fetch_card_events(card_id, after);
        if (!alive || rows.length === 0) return;
        after = rows[rows.length - 1].id;
        setEvents((prev) => [...prev, ...rows]);
      } catch {
        // backend unreachable: keep what we already have
      }
    };
    load();
    if (!live) return () => { alive = false; };
    const timer = setInterval(load, RUN_EVENT_POLL_MS);
    return () => { alive = false; clearInterval(timer); };
  }, [card_id, live]);
  if (!run && !live && events.length === 0) return null;
  const stream = (
    <ol>
      {events.map((e) => (
        <li key={e.id} className={`run-stage run-ok run-ev run-ev-${e.kind}`}>
          <span className="run-stage-body">
            <strong>{RUN_EVENT_LABEL[e.kind] ?? e.kind}</strong>
            <pre className="run-output">{e.text}</pre>
          </span>
        </li>
      ))}
    </ol>
  );
  if (live) {
    const p = card ? progress_of(card) : null;
    return (
      <section className="run-timeline" aria-label="run log">
        <h3>
          <Loader2 size={13} className="spin" /> run
          <span className="run-ok-text">running{card?.last_agent ? ` · ${card.last_agent}` : ""}</span>
        </h3>
        {p && (
          <p className="run-stage-stage">
            {p.retry ? "retry" : "step"} {p.round}/{p.rounds} · {p.last_tool}
          </p>
        )}
        {stream}
      </section>
    );
  }
  return (
    <>
      <RunTimeline run={run} />
      {events.length > 0 && (
        <section className="run-timeline" aria-label="agent stream">{stream}</section>
      )}
    </>
  );
}

function RunTimeline({ run }: { run: CardRun | null }) {
  if (!run) return null;
  return (
    <section className="run-timeline" aria-label="run log">
      <h3>
        <Play size={13} /> run{" "}
        <span className={run.status === "ok" ? "run-ok-text" : "run-failed-text"}>{STAGE_LABEL[run.status]}</span>
      </h3>
      <ol>
        {(run.stages ?? []).map((s, i) => (
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

/// Board-level strip of all in-flight runs across projects; empty → nothing.
function ActiveStrip({ runs, on_open }: { runs: ActiveRun[]; on_open: (card: Card) => void }) {
  if (runs.length === 0) return null;
  return (
    <nav className="active-strip" aria-label="runs in flight">
      {runs.map((r) => {
        const agent = r.card.last_agent ?? r.card.agent_name;
        return (
          <button
            key={r.card.id}
            className="active-chip"
            onClick={() => on_open(r.card)}
            title={`card #${r.card.id} — open`}
          >
            <Loader2 size={11} className="spin" />
            <span className="active-chip-agent">{agent ?? "?"}</span>
            <span className="active-chip-title">#{r.card.id} {r.card.title}</span>
            {r.progress && (
              <span className="active-chip-progress">
                step {r.progress.round}/{r.progress.rounds}
              </span>
            )}
          </button>
        );
      })}
    </nav>
  );
}
