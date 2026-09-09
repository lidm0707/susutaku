import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { ArrowLeft, ArrowRight, Bot, CalendarClock, CheckCircle2, Clock, Flag, Loader2, Play, Plus, Save, Trash2, User, Workflow, XCircle } from "lucide-react";
import {
  clear_token,
  add_comment,
  chat,
  create_card,
  fetch_agents,
  fetch_cards,
  fetch_comments,
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
  type Comment,
  type Pipeline,
  type UserInfo,
} from "../lib.js";
import { Modal, SlideOver } from "../ui/Overlay.js";
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
  const [dThinking, setDThinking] = useState(false);
  const commentsEnd = useRef<HTMLDivElement | null>(null);
  const [dragOver, setDragOver] = useState<ColumnId | null>(null);
  const [draggingId, setDraggingId] = useState<number | null>(null);
  const [runningId, setRunningId] = useState<number | null>(null);

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
    setDPriority((card.priority as Priority) || PRIORITIES[1]);
    setDDeadline(card.deadline || "");
    setDComments([]);
    setDCommentBody("");
    setDTab("comments");
    setDThinking(false);
    fetch_agents().then(setSavedAgents).catch(() => setSavedAgents([]));
    fetch_users().then(setUsers).catch(() => setUsers([]));
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
          fields.deadline
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

  async function pick_detail_pipeline(id: string) {
    const pipe_id = id === "" ? null : Number(id);
    const updated = await patch_detail({ pipeline_id: pipe_id });
    if (updated) setDetail({ ...detail!, pipeline_id: pipe_id });
  }

  async function pick_detail_schedule(expr: string) {
    const updated = await patch_detail({ cron: expr || null });
    if (updated) setDetail({ ...detail!, cron: expr || null });
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

  return (
    <main className="chat kanban-page">
      <header>
        <h1>kanban</h1>
        <span className="sub">{cards.length} task{cards.length === 1 ? "" : "s"}</span>
      </header>
      <form className="kanban-add" onSubmit={add}>
        <input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder={project_id == null ? "create a workspace + project first…" : "new task title…"}
          disabled={project_id == null}
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
            <div className="kanban-detail-field">
              <label htmlFor="kanban-detail-pipeline"><Workflow size={13} /> pipeline</label>
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
              <label htmlFor="kanban-detail-schedule"><Clock size={13} /> time trigger</label>
              <select
                id="kanban-detail-schedule"
                className="kanban-select"
                value={detail?.cron || ""}
                onChange={(e) => pick_detail_schedule(e.target.value)}
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
            </div>
          </section>
        </div>
      </SlideOver>
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
