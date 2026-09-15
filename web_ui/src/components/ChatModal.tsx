import { useEffect, useRef, useState } from "react";
import { useLocation } from "react-router-dom";
import { Bot, Brain, Camera, Check, CircleDot, Code2, Copy, Crosshair, ExternalLink, Eye, Link2, MessageSquarePlus, PanelRight, Send, Wrench, X } from "lucide-react";
import {
  API_BASE,
  ApiError,
  cancel_chat_run,
  chat_codex,
  chat_zai,
  chat_zai_stream,
  create_chat_thread,
  delete_chat_thread,
  fetch_agent_machine,
  fetch_agents,
  fetch_chat_messages,
  fetch_chat_threads,
  fetch_cards,
  fetch_codex_models,
  fetch_cronjobs,
  fetch_models,
  fetch_system_prompt,
  get_token,
  pretty_name,
  select_model,
  PARAM_CARD,
  PARAM_PROJECT,
  type Agent,
  type ChatReply,
  type ChatStreamEvent,
  type ChatToolUse,
  type CodexModel,
  type CronJob,
  type ModelInfo,
  type PromptSection,
} from "../lib.js";
import { Modal } from "../ui/Overlay.js";
import AgentReview from "./AgentReview.js";
import { use_projects } from "./ProjectContext.js";
import GraphView from "./GraphView.tsx";
import { parse_plot_spec } from "../features/graph.js";
import CapscreenModal from "./CapscreenModal.js";
import { image_files_to_data_urls } from "../features/capscreen.js";
import { use_card_created, type CardEvent } from "../features/card_bus.js";

const MAX_TOKENS = 512;

const RATE_LIMIT_STATUS = 429;

const ERR_DETAIL_LEN = 200;

const THINK_OPEN = "<think>";
const THINK_CLOSE = "</think>";

const TOAST_MS = 4000;

const THREAD_TITLE_LEN = 24;

const SCROLL_STICK_PX = 48;

const CARD_MIME = "application/x-susutaku-card";

// human-readable failure reason for a chat run: rate limits, usage/quota
// errors and backend bodies otherwise just look like "run failed"
function describe_error(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.status === RATE_LIMIT_STATUS) return "rate limited — wait for the window to reset and retry";
    const body = e.message.slice(String(e.status).length).trim();
    const detail = body ? `: ${body.slice(0, ERR_DETAIL_LEN)}` : "";
    return `failed (${e.status})${detail}`;
  }
  const msg = String((e as Error)?.message ?? e);
  return msg ? msg.slice(0, ERR_DETAIL_LEN) : "run failed";
}

// crypto.randomUUID exists only in secure contexts; the UI is also served
// over plain http on the docker network, so fall back to a random id
function make_run_id(): string {
  if (typeof crypto.randomUUID === "function") return crypto.randomUUID();
  return `run-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
const CARD_MENTION_RE = /#card:(\d+)/g;
// board tool summary the backend emits after an agent creates a card
const CARD_CREATED_RE = /^card (\d+) created in project (\d+): (.+)$/;
const HTML_LANG = "html";
const HTML_FRAME_HEIGHT = "220px";

const DOCK_KEY = "chat_dock";

// per-thread composer drafts survive a page refresh (local threads have no
// server side store, so localStorage is the only persistence they get)
const DRAFT_KEY = "chat_drafts_v1";

function thread_key(t: Thread | undefined, project: number | null): string | null {
  if (!t) return null;
  return t.server_id === null ? `l${project ?? 0}:${t.id}` : `s${t.server_id}`;
}

function read_draft(key: string | null): string {
  if (!key) return "";
  try {
    return JSON.parse(localStorage.getItem(DRAFT_KEY) ?? "{}")[key] ?? "";
  } catch {
    return "";
  }
}

function write_draft(key: string | null, text: string) {
  if (!key) return;
  try {
    const all = JSON.parse(localStorage.getItem(DRAFT_KEY) ?? "{}");
    if (text) all[key] = text;
    else delete all[key];
    localStorage.setItem(DRAFT_KEY, JSON.stringify(all));
  } catch {}
}

// per-project last active thread key, so switching project and coming back
// reopens the ongoing conversation instead of a blank "thread 1"
const LAST_THREAD_KEY = "chat_last_threads_v1";

function read_last_key(project: number | null): string | null {
  try {
    return JSON.parse(localStorage.getItem(LAST_THREAD_KEY) ?? "{}")[`p${project ?? 0}`] ?? null;
  } catch {
    return null;
  }
}

function write_last_key(project: number | null, key: string | null) {
  if (!key) return;
  try {
    const all = JSON.parse(localStorage.getItem(LAST_THREAD_KEY) ?? "{}");
    all[`p${project ?? 0}`] = key;
    localStorage.setItem(LAST_THREAD_KEY, JSON.stringify(all));
  } catch {}
}

// url query param that opens the chat docked on a shared thread (?chat=<server id>)
export const CHAT_PARAM = "chat";
const DOCK_RIGHT = "right";
const DOCK_W_KEY = "chat_dock_w_v2";
const DOCK_W_MIN = 320;
const DOCK_W_MAX_FRAC = 0.9;
const DOCK_W_DEFAULT = "min(40vw, 94vw)";

const dock_listeners = new Set<() => void>();

function read_dock(): boolean {
  const v = localStorage.getItem(DOCK_KEY);
  return v === null ? true : v === DOCK_RIGHT;
}

function read_dock_w(): string {
  const v = Number(localStorage.getItem(DOCK_W_KEY));
  return v >= DOCK_W_MIN ? `${v}px` : DOCK_W_DEFAULT;
}

// shared so the app layout can reserve space while the chat is docked right
export function use_chat_docked(): boolean {
  const [docked, set_docked] = useState(read_dock);
  useEffect(() => {
    const fn = () => set_docked(read_dock());
    dock_listeners.add(fn);
    window.addEventListener("storage", fn);
    return () => {
      dock_listeners.delete(fn);
      window.removeEventListener("storage", fn);
    };
  }, []);
  return docked;
}

const PAGE_LABELS: Record<string, string> = {
  "/task": "task",
  "/routine": "routine",
  "/attachments": "attachments",
  "/agents": "agents",
  "/settings": "settings",
  "/sandbox": "sandbox",
  "/": "login",
};

export function page_label(pathname: string): string {
  return PAGE_LABELS[pathname] ?? "task";
}

const ROUTINE_CONTEXT_MAX = 20;

function fmt_run(unix: number): string {
  return new Date(unix * 1000).toISOString().replace("T", " ").slice(0, 16) + " UTC";
}

async function page_data(pathname: string): Promise<string> {
  if (pathname !== "/routine") return "";
  let jobs: CronJob[] = [];
  try {
    jobs = await fetch_cronjobs();
  } catch {
    return "";
  }
  if (!jobs.length) return "routines: none defined";
  const lines = jobs.slice(0, ROUTINE_CONTEXT_MAX).map(
    (j) =>
      `- card #${j.card_id} "${j.title}": schedule "${j.cron}"` +
      (j.next_run ? `, next run ${fmt_run(j.next_run)}` : ", not scheduled in queue"),
  );
  return `routines (card schedules):\n${lines.join("\n")}`;
}

function split_thinking(text: string): { thinking: string; reply: string } {
  const open = text.indexOf(THINK_OPEN);
  const close = text.indexOf(THINK_CLOSE);
  if (open === -1) return { thinking: "", reply: text.trim() };
  const thinking = (
    open + THINK_OPEN.length < close ? text.slice(open + THINK_OPEN.length, close) : text.slice(0, open)
  ).trim();
  const reply = (text.slice(0, open) + text.slice(close + THINK_CLOSE.length)).trim();
  return { thinking, reply };
}

const FENCE_RE = /```([a-zA-Z0-9_-]*)\n?([\s\S]*?)(?:```|$)/g;
const INLINE_CODE_RE = /`([^`\n]+)`/g;
const TABLE_LINE_RE = /^\s*\|.*\|\s*$/;
const TABLE_SEP_RE = /^\s*\|(\s*:?-+:?\s*\|)+\s*$/;

interface TableBlock {
  kind: "table";
  rows: string[][];
}

interface PlainBlock {
  kind: "plain";
  body: string;
}

function split_cells(line: string): string[] {
  const t = line.trim();
  const inner = t.startsWith("|") ? t.slice(1) : t;
  const core = inner.endsWith("|") ? inner.slice(0, -1) : inner;
  return core.split("|").map((c) => c.trim());
}

function split_table_blocks(body: string): (TableBlock | PlainBlock)[] {
  const lines = body.split("\n");
  const blocks: (TableBlock | PlainBlock)[] = [];
  let plain: string[] = [];
  const flush = () => {
    if (plain.length) {
      blocks.push({ kind: "plain", body: plain.join("\n") });
      plain = [];
    }
  };
  let i = 0;
  while (i < lines.length) {
    const sep = lines[i + 1];
    if (TABLE_LINE_RE.test(lines[i]) && sep !== undefined && TABLE_SEP_RE.test(sep)) {
      flush();
      const rows: string[][] = [split_cells(lines[i])];
      i += 2;
      while (i < lines.length && TABLE_LINE_RE.test(lines[i])) {
        rows.push(split_cells(lines[i]));
        i += 1;
      }
      blocks.push({ kind: "table", rows });
    } else {
      plain.push(lines[i]);
      i += 1;
    }
  }
  flush();
  return blocks;
}

function MsgTable({ rows }: { rows: string[][] }) {
  const [head, ...body] = rows;
  return (
    <div className="msg-table-wrap">
      <table className="msg-table">
        <thead>
          <tr>{head.map((c, j) => <th key={j}>{c}</th>)}</tr>
        </thead>
        <tbody>
          {body.map((r, j) => (
            <tr key={j}>{r.map((c, k) => <td key={k}>{c}</td>)}</tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function TextBody({ body }: { body: string }) {
  return (
    <>
      {split_table_blocks(body).map((b, i) =>
        b.kind === "table" ? (
          <MsgTable key={i} rows={b.rows} />
        ) : (
          <InlineText key={i} body={b.body} />
        )
      )}
    </>
  );
}
const COPY_RESET_MS = 1500;

interface CodeSegment {
  kind: "code";
  lang: string;
  body: string;
}

interface TextSegment {
  kind: "text";
  body: string;
}

function split_segments(text: string): (CodeSegment | TextSegment)[] {
  const segments: (CodeSegment | TextSegment)[] = [];
  let last = 0;
  for (const m of text.matchAll(FENCE_RE)) {
    const at = m.index;
    if (at === undefined) break;
    if (at > last) segments.push({ kind: "text", body: text.slice(last, at) });
    segments.push({ kind: "code", lang: m[1], body: m[2] });
    last = at + m[0].length;
  }
  if (last < text.length) segments.push({ kind: "text", body: text.slice(last) });
  return segments;
}

function InlineText({ body }: { body: string }) {
  const parts: (string | JSX.Element)[] = [];
  let last = 0;
  for (const m of body.matchAll(INLINE_CODE_RE)) {
    const at = m.index ?? 0;
    if (at > last) parts.push(body.slice(last, at));
    parts.push(<code key={at}>{m[1]}</code>);
    last = at + m[0].length;
  }
  if (last < body.length) parts.push(body.slice(last));
  return <p>{parts}</p>;
}

function CodeBlock({ lang, body }: { lang: string; body: string }) {
  const [copied, set_copied] = useState(false);
  const copy = () => {
    navigator.clipboard.writeText(body).then(() => {
      set_copied(true);
      setTimeout(() => set_copied(false), COPY_RESET_MS);
    });
  };
  return (
    <div className="msg-code">
      <div className="msg-code-head">
        <span>{lang || "code"}</span>
        <button type="button" onClick={copy} aria-label="copy code">
          {copied ? <Check size={12} /> : <Copy size={12} />}
        </button>
      </div>
      <pre><code>{body}</code></pre>
    </div>
  );
}

const PLOT_LANG = "plot";

function PlotFence({ body }: { body: string }) {
  const spec = parse_plot_spec(body);
  if (!spec) return <CodeBlock lang={PLOT_LANG} body={body} />;
  return (
    <div className="msg-plot">
      <GraphView spec={spec} />
    </div>
  );
}

function HtmlFence({ body }: { body: string }) {
  const [raw, set_raw] = useState(false);
  return (
    <div className="msg-code msg-html">
      <div className="msg-code-head">
        <span>{HTML_LANG}</span>
        <button
          type="button"
          onClick={() => set_raw((v) => !v)}
          aria-pressed={raw}
          title={raw ? "show live preview" : "show raw html"}
        >
          {raw ? <Eye size={12} /> : <Code2 size={12} />}
          {raw ? "preview" : "raw html"}
        </button>
      </div>
      {raw ? (
        <pre><code>{body}</code></pre>
      ) : (
        <iframe
          className="msg-html-frame"
          sandbox=""
          srcDoc={body}
          title="html preview"
          style={{ height: HTML_FRAME_HEIGHT }}
        />
      )}
    </div>
  );
}

function CardChip({ card }: { card: CardEvent }) {
  const open = () =>
    window.open(
      `/task?${PARAM_PROJECT}=${card.project_id ?? ""}&${PARAM_CARD}=${card.id}`,
      "_blank"
    );
  return (
    <button type="button" className="card-chip" onClick={open} title="open card on the task board">
      <ExternalLink size={12} />
      card #{card.id} · {card.title}
    </button>
  );
}

function MessageText({ text }: { text: string }) {
  const segments = split_segments(text);
  return (
    <div className="msg-body">
      {segments.map((s, i) =>
        s.kind === "code" ? (
          s.lang === PLOT_LANG ? (
            <PlotFence key={i} body={s.body} />
          ) : s.lang === HTML_LANG ? (
            <HtmlFence key={i} body={s.body} />
          ) : (
            <CodeBlock key={i} lang={s.lang} body={s.body} />
          )
        ) : (
          <TextBody key={i} body={s.body} />
        ),
      )}
    </div>
  );
}

// a message parked while the thread is still generating
interface QueuedMsg {
  text: string;
  images: string[];
}

interface Msg {
  id: number;
  role: "user" | "assistant";
  text: string;
  pending?: boolean;
  failed?: boolean;
  error?: string;
  thinking?: string;
  agent_name?: string;
  model?: string;
  prompt_tps?: number;
  tps?: number;
  /// annotated screenshot attached to this message
  image?: string;
  /// task card created in this turn (agent tool call or board action)
  card?: CardEvent;
  tools?: ChatToolUse[];
  memories?: string[];
}

interface Thread {
  id: number;
  title: string;
  messages: Msg[];
  server_id: number | null;
  loaded: boolean;
  updated_at: number;
  // run finished (or failed) while another thread was on screen
  unread?: boolean;
}

export default function ChatModal({ open, on_close }: { open: boolean; on_close: () => void }) {
  const { pathname } = useLocation();
  const { project_id } = use_projects();
  const [threads, setThreads] = useState<Thread[]>([
    { id: 0, title: "thread 1", messages: [], server_id: null, loaded: true, updated_at: Date.now() },
  ]);
  const [activeId, setActiveId] = useState(0);
  const [input, setInput] = useState("");
  const inputRef = useRef<HTMLTextAreaElement | null>(null);
  // runs are isolated per thread: several threads can generate at once
  const [busyTids, setBusyTids] = useState<number[]>([]);
  const busyTidsRef = useRef<number[]>([]);
  // thread id -> backend run ids of its in-flight streamed runs (for cancel)
  const runIdsRef = useRef<Record<number, string[]>>({});
  const threadBusy = busyTids.includes(activeId);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [codexModels, setCodexModels] = useState<CodexModel[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [selectedIds, setSelectedIds] = useState<number[]>([]);
  const [sysPrompt, setSysPrompt] = useState("");
  const [searchMode, setSearchMode] = useState<"off" | "auto" | "on">("auto");
  const [pickerOpen, setPickerOpen] = useState(false);
  const [docked, setDocked] = useState(read_dock);
  const dock_w = useRef(read_dock_w());
  const [error, setError] = useState("");
  const [toast, setToast] = useState("");
  // focus toggle: OFF (default) sends the bare message, ON prefixes page/card context
  const [focusOn, setFocusOn] = useState(false);
  // server thread id taken from a shared ?chat= link, resolved once threads load
  const [sharedId, setSharedId] = useState(
    () => Number(new URLSearchParams(window.location.search).get(CHAT_PARAM)) || 0,
  );
  // agent name -> machine it currently runs on ("" = not running anywhere).
  const [machineByAgent, setMachineByAgent] = useState<Record<string, string>>({});
  const [capscreenOpen, setCapscreenOpen] = useState(false);
  const [pendingImages, setPendingImages] = useState<string[]>([]);
  const [inputDrag, setInputDrag] = useState(false);
  const nextId = useRef(1);

  // per-thread in-flight run counter: runs can overlap when a send interrupts
  // the current reply and starts a new one right away
  const busyCount = useRef(new Map<number, number>());
  // per-thread queued message: sending while busy parks the text here instead
  // of interrupting — Send Now interrupts, run completion auto-flushes
  const [queued, setQueued] = useState<Record<number, QueuedMsg | undefined>>({});
  const queuedRef = useRef(queued);
  queuedRef.current = queued;
  function bump_busy(tid: number, d: number) {
    const c = (busyCount.current.get(tid) ?? 0) + d;
    if (c <= 0) busyCount.current.delete(tid);
    else busyCount.current.set(tid, c);
    busyTidsRef.current = [...busyCount.current.keys()];
    setBusyTids(busyTidsRef.current);
  }

  const nextThreadId = useRef(1);
  const loadingThreads = useRef(new Set<number>());
  const pickerRef = useRef<HTMLDivElement | null>(null);
  const logRef = useRef<HTMLElement | null>(null);
  const stickBottom = useRef(true);
  const active = threads.find((t) => t.id === activeId) ?? threads[0];
  const messages = active.messages;
  const sortedThreads = [...threads].sort(
    (a, b) => b.updated_at - a.updated_at || b.id - a.id
  );
  const selected = agents.filter((a) => selectedIds.includes(a.id as number));
  const activeKey = thread_key(threads.find((t) => t.id === activeId), project_id);

  // restore the draft of the active thread (covers refresh on local threads,
  // which have no server side storage to reload from)
  useEffect(() => {
    setInput(read_draft(activeKey));
    const el = inputRef.current;
    if (el) el.style.height = "auto";
  }, [activeKey]);

  // remember the last active thread per project so a project switch and back
  // reopens the ongoing conversation instead of a blank thread
  useEffect(() => {
    if (activeKey) write_last_key(project_id, activeKey);
  }, [activeKey, project_id]);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(""), TOAST_MS);
    return () => clearTimeout(t);
  }, [toast]);

  useEffect(() => {
    if (!pickerOpen) return;
    const on_doc_click = (e: MouseEvent) => {
      if (pickerRef.current && !pickerRef.current.contains(e.target as Node)) setPickerOpen(false);
    };
    document.addEventListener("mousedown", on_doc_click);
    return () => document.removeEventListener("mousedown", on_doc_click);
  }, [pickerOpen]);

  useEffect(() => {
    if (!open) return;
    fetch_models().then(setModels).catch(() => {});
    fetch_codex_models().then(setCodexModels).catch(() => {});
    fetch_agents()
      .then((list: Agent[]) => {
        const real = list.filter((a) => a.id !== "new");
        setAgents(real);
        setSelectedIds((cur) => (cur.length ? cur : real.length ? [real[0].id as number] : []));
      })
      .catch(() => {});
    fetch_system_prompt().then(setSysPrompt).catch(() => {});
    fetch_chat_threads(project_id)
      .then((rows) =>
        setThreads((ts) => {
          const locals = ts.filter((t) => t.server_id === null);
          const nextLocalId = Math.max(0, ...locals.map((t) => t.id)) + 1;
          nextThreadId.current = Math.max(nextThreadId.current, nextLocalId);
          // reuse the existing thread object when one already tracks this
          // server row: keeps its local id valid so in-flight streaming
          // (patch_thread/patch_msg by id) survives a project switch and back
          const server: Thread[] = rows.map((r) => {
            const known = ts.find((t) => t.server_id === r.id);
            if (known)
              return {
                ...known,
                title: known.title || r.title || `thread ${r.id}`,
                updated_at: Math.max(known.updated_at, Date.parse(r.updated_at) || 0),
              };
            return {
              id: nextThreadId.current++,
              title: r.title || `thread ${r.id}`,
              messages: [],
              server_id: r.id,
              loaded: false,
              updated_at: Date.parse(r.updated_at) || 0,
            };
          });
          return [...server, ...locals];
        })
      )
      .catch(() => {})
      .finally(() => setThreadsLoadedFor(project_id));
  }, [open, project_id]);

  const [threadsLoadedFor, setThreadsLoadedFor] = useState<number | null | undefined>(undefined);

  // switching project resets to a fresh thread of the new project scope,
  // but threads with in-flight replies are kept so runs are not lost
  useEffect(() => {
    if (!open) return;
    setThreadsLoadedFor(undefined);
    setActiveId(0);
    setThreads((ts) => [
      { id: nextThreadId.current++, title: "thread 1", messages: [], server_id: null, loaded: true, updated_at: Date.now() },
      ...ts.filter((t) => t.messages.some((m) => m.pending)),
    ]);
  }, [project_id]);

  // once the project's threads are loaded, jump back to the thread that was
  // active the last time this project was open (no-op if it was deleted)
  useEffect(() => {
    if (!open || threadsLoadedFor !== project_id) return;
    const want = read_last_key(project_id);
    if (!want || threads.find((t) => t.id === activeId && thread_key(t, project_id) === want)) return;
    const t = threads.find((x) => thread_key(x, project_id) === want);
    if (t) setActiveId(t.id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, project_id, threadsLoadedFor, threads]);

  // re-opening the chat (or the panel re-gaining focus after a page change)
  // pulls fresh messages for the active thread so replies that finished while
  // hidden show up instead of staying on the pending dots forever
  useEffect(() => {
    if (!open) return;
    const t = threads.find((x) => x.id === activeId);
    if (
      !t ||
      t.server_id === null ||
      busyTidsRef.current.includes(t.id) ||
      loadingThreads.current.has(t.id)
    )
      return;
    loadingThreads.current.add(t.id);
    fetch_chat_messages(t.server_id)
      .then((rows) =>
        setThreads((ts) =>
          ts.map((x) =>
            x.id === t.id
              ? {
                  ...x,
                  loaded: true,
                  messages: [
                    ...rows.map((m) =>
                      m.role === "user"
                        ? { id: m.id, role: "user" as const, text: m.text }
                        : { id: m.id, role: "assistant" as const, text: m.text }
                    ),
                    // keep optimistic in-flight bubbles at the end
                    ...x.messages.filter((m) => m.pending),
                  ],
                }
              : x
          )
        )
      )
      .catch(() => {})
      .finally(() => loadingThreads.current.delete(t.id));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // Fetch a server thread's messages the first time it is opened.
  useEffect(() => {
    const t = threads.find((x) => x.id === activeId);
    if (!t || t.server_id === null || t.loaded || loadingThreads.current.has(t.id)) return;
    loadingThreads.current.add(t.id);
    fetch_chat_messages(t.server_id)
      .then((rows) =>
        setThreads((ts) =>
          ts.map((x) =>
            x.id === t.id
              ? {
                  ...x,
                  loaded: true,
                  messages: rows.map((m) =>
                    m.role === "user"
                      ? { id: m.id, role: "user" as const, text: m.text }
                      : { id: m.id, role: "assistant" as const, text: m.text }
                  ),
                }
              : x
          )
        )
      )
      .catch(() => {})
      .finally(() => loadingThreads.current.delete(t.id));
  }, [activeId, threads]);

  useEffect(() => {
    if (!open) return;
    agents.forEach((a) => {
      if (a.name in machineByAgent) return;
      fetch_agent_machine(a.name)
        .then((w) => setMachineByAgent((m) => ({ ...m, [a.name]: w.machine })))
        .catch(() => setMachineByAgent((m) => ({ ...m, [a.name]: "" })));
    });
  }, [open, agents]);

  // once threads are loaded, jump to the thread named by a shared ?chat= link
  useEffect(() => {
    if (!sharedId) return;
    const t = threads.find((x) => x.server_id === sharedId);
    if (!t) return;
    setActiveId(t.id);
    setSharedId(0);
  }, [sharedId, threads]);

  // keep ?chat=<server id> in the url pointing at the active thread so a
  // page refresh reopens the same chat instead of falling back to a new one
  useEffect(() => {
    const t = threads.find((x) => x.id === activeId);
    const sid = t?.server_id ?? null;
    const params = new URLSearchParams(window.location.search);
    if (String(sid ?? "") === params.get(CHAT_PARAM)) return;
    if (sid === null) params.delete(CHAT_PARAM);
    else params.set(CHAT_PARAM, String(sid));
    const q = params.toString();
    window.history.replaceState(null, "", window.location.pathname + (q ? `?${q}` : ""));
  }, [activeId, threads]);

  function toggle_dock() {
    setDocked((d) => {
      if (d) localStorage.removeItem(DOCK_KEY);
      else localStorage.setItem(DOCK_KEY, DOCK_RIGHT);
      dock_listeners.forEach((fn) => fn());
      return !d;
    });
  }

  function apply_dock_w(w: string) {
    dock_w.current = w;
    document.documentElement.style.setProperty("--chat-dock-w", w);
  }

  useEffect(() => {
    if (!docked) return;
    apply_dock_w(dock_w.current);
    return () => {
      document.documentElement.style.removeProperty("--chat-dock-w");
    };
  }, [docked]);

  function start_resize(e: React.MouseEvent) {
    e.preventDefault();
    document.documentElement.classList.add("chat-dock-resizing");
    const on_move = (ev: MouseEvent) => {
      const max = Math.floor(window.innerWidth * DOCK_W_MAX_FRAC);
      const w = Math.min(Math.max(window.innerWidth - ev.clientX, DOCK_W_MIN), max);
      apply_dock_w(`${w}px`);
    };
    const on_up = () => {
      const px = parseInt(dock_w.current, 10);
      localStorage.setItem(DOCK_W_KEY, Number.isFinite(px) ? String(px) : "");
      document.documentElement.classList.remove("chat-dock-resizing");
      window.removeEventListener("mousemove", on_move);
      window.removeEventListener("mouseup", on_up);
    };
    window.addEventListener("mousemove", on_move);
    window.addEventListener("mouseup", on_up);
  }

  function on_log_scroll() {
    const el = logRef.current;
    if (!el) return;
    stickBottom.current =
      el.scrollHeight - el.scrollTop - el.clientHeight <= SCROLL_STICK_PX;
  }

  useEffect(() => {
    const el = logRef.current;
    if (!el || !stickBottom.current) return;
    el.scrollTop = el.scrollHeight;
  }, [messages, activeId]);

  function patch_thread(id: number, fn: (msgs: Msg[]) => Msg[]) {
    setThreads((ts) =>
      ts.map((t) => (t.id === id ? { ...t, messages: fn(t.messages), updated_at: Date.now() } : t))
    );
  }

  // patch a single message by id across threads (streamed reply updates)
  function patch_msg(id: number, over: Partial<Msg>) {
    setThreads((ts) =>
      ts.map((t) =>
        t.messages.some((m) => m.id === id)
          ? {
              ...t,
              messages: t.messages.map((m) => (m.id === id ? { ...m, ...over } : m)),
              updated_at: Date.now(),
            }
          : t
      )
    );
  }


  function set_title_from(id: number, text: string) {
    setThreads((ts) =>
      ts.map((t) =>
        t.id === id && t.messages.length === 0
          ? { ...t, title: text.slice(0, THREAD_TITLE_LEN) || t.title }
          : t
      )
    );
  }

  function new_thread() {
    const id = nextThreadId.current++;
    setThreads((ts) => [
      ...ts,
      { id, title: `thread ${id + 1}`, messages: [], server_id: null, loaded: true, updated_at: Date.now() },
    ]);
    setActiveId(id);
    setError("");
  }

  function share_url(t: Thread): string | null {
    if (t.server_id === null) return null;
    return `${window.location.origin}${window.location.pathname}?${CHAT_PARAM}=${t.server_id}`;
  }

  async function copy_share(t: Thread) {
    const url = share_url(t);
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
      setToast("link copied — anyone with an account can open this chat");
    } catch {
      setError("could not copy link");
    }
  }

  function close_thread(id: number) {
    const gone = threads.find((t) => t.id === id);
    if (gone?.server_id !== null && gone?.server_id !== undefined) {
      delete_chat_thread(gone.server_id).catch(() => {});
    }
    setThreads((ts) => {
      const rest = ts.filter((t) => t.id !== id);
      if (!rest.length) {
        const fresh = {
          id: nextThreadId.current++,
          title: "thread 1",
          messages: [] as Msg[],
          server_id: null,
          loaded: true,
          updated_at: Date.now(),
        };
        setActiveId(fresh.id);
        return [fresh];
      }
      if (id === activeId) setActiveId(rest[rest.length - 1].id);
      return rest;
    });
  }

  function toggle_agent(id: number) {
    setSelectedIds((cur) =>
      cur.includes(id) ? cur.filter((x) => x !== id) : [...cur, id]
    );
  }

  function card_mentions(text: string): number[] {
    const ids = new Set<number>();
    for (const m of text.matchAll(CARD_MENTION_RE)) ids.add(Number(m[1]));
    return [...ids];
  }

  async function card_context(text: string): Promise<string> {
    const ids = card_mentions(text);
    if (!ids.length) return "";
    const cards = await fetch_cards(null).catch(() => []);
    const lines = cards
      .filter((c) => ids.includes(c.id))
      .map((c) => `card #${c.id}: ${c.title}${c.description ? ` — ${c.description}` : ""}`);
    return lines.length ? `mentioned cards:\n${lines.join("\n")}` : "";
  }

  function agent_sections(a: Agent): PromptSection[] {
    const instructions = [a.persona, a.prompt].filter((s) => s.trim()).join("\n\n");
    return [
      { role: "system", body: sysPrompt },
      { role: "instructions", body: instructions },
    ].filter((s) => s.body.trim());
  }

  async function send_agent(
    a: Agent,
    text: string,
    thread_id: number | null,
    image?: string,
    on_event?: (ev: ChatStreamEvent) => void,
    run_id?: string
  ): Promise<ChatReply> {
    let codex = codexModels;
    if (!codex.length) {
      codex = await fetch_codex_models().catch(() => []);
      setCodexModels(codex);
    }
    if (codex.some((m) => m.id === a.model)) return chat_codex(text, a.model);
    const local = models.find((m) => m.name === a.model);
    if (local) {
      if (!local.selected) await select_model(a.model);
      const headers: Record<string, string> = { "Content-Type": "application/json" };
      // the board tools run with the caller's token (editor guard)
      const token = get_token();
      if (token) headers.Authorization = `Bearer ${token}`;
      const res = await fetch(`${API_BASE}/api/chat`, {
        method: "POST",
        headers,
        body: JSON.stringify({
          message: text,
          max_tokens: MAX_TOKENS,
          search: searchMode,
          agent: a.name,
          thread_id: thread_id ?? undefined,
          // first #card:N mention receives tool artifacts as card resources
          card_id: card_mentions(text)[0],
        }),
      });
      return res.json();
    }
    // agents flagged receive_images=false get text only — the backend rejects
    // images for them with 403
    const img = a.receive_images === false ? undefined : image;
    if (on_event) {
      return chat_zai_stream(
        text,
        a.model,
        agent_sections(a),
        a.name,
        thread_id ?? undefined,
        img,
        on_event,
        run_id
      );
    }
    return chat_zai(text, a.model, agent_sections(a), a.name, thread_id ?? undefined, img);
  }

  async function send(e?: React.FormEvent | React.KeyboardEvent) {
    e?.preventDefault();
    const text = input.trim();
    if (!text && !pendingImages.length) return;
    if (!selected.length) {
      setError("no agent — pick one with the + button");
      return;
    }
    const tid = activeId;
    // one image per message on the wire: the first rides with the text, the
    // rest go out sequentially as image-only follow-ups
    const images = [...pendingImages];
    setInput("");
    write_draft(thread_key(threads.find((t) => t.id === tid), project_id), "");
    setPendingImages([]);
    await dispatch(text, images, tid);
  }

  async function dispatch(text: string, images: string[], tid: number) {
    // while the thread is busy the message queues instead of interrupting:
    // it flushes automatically when the run ends, or immediately via Send Now
    if (busyTidsRef.current.includes(tid)) {
      setQueued((q) => ({ ...q, [tid]: { text, images } }));
      setToast("queued — send now to interrupt the current reply");
      return;
    }
    await run_send(text, images[0] ?? null, tid);
    for (const img of images.slice(1)) {
      await run_send("", img, tid);
    }
  }

  function clear_queued(tid: number) {
    setQueued((q) => {
      const { [tid]: _drop, ...rest } = q;
      return rest;
    });
  }

  // interrupt the current reply and send the queued message right away
  async function send_now(tid: number) {
    const q = queuedRef.current[tid];
    if (!q) return;
    clear_queued(tid);
    await stop_thread(tid);
    // bypass dispatch: the interrupted run is still winding down so the thread
    // still reads busy — overlapping here is safe (per-run cancel ids)
    await run_send(q.text, q.images[0] ?? null, tid);
    for (const img of q.images.slice(1)) {
      await run_send("", img, tid);
    }
  }

  async function run_send(text: string, attachedImage: string | null, tid: number) {
    const thread = threads.find((t) => t.id === tid);
    setError("");
    bump_busy(tid, +1);
    // cancel ids of this run only — an overlapping run on the same thread
    // keeps its own ids so a later interrupt still reaches the right stream
    const myRunIds: string[] = [];
    if (tid === activeIdRef.current) stickBottom.current = true;
    set_title_from(tid, text || "image");
    const userId = nextId.current++;
    const replyIds = selected.map(() => nextId.current++);
    try {
    const image = attachedImage;
    patch_thread(tid, (m) => [
      ...m,
      { id: userId, role: "user", text, image: attachedImage ?? undefined },
      ...selected.map((a, i) => ({
        id: replyIds[i],
        role: "assistant" as const,
        text: "",
        pending: true,
        agent_name: a.name,
        model: a.model,
      })),
    ]);
    const data = focusOn ? await page_data(pathname) : "";
    const cards = focusOn ? await card_context(text) : "";
    const contexted = focusOn
      ? `[context: user is currently on the ${page_label(pathname)} page${data ? `\n${data}` : ""}${cards ? `\n${cards}` : ""}]${image ? "\n[a screenshot is attached]" : ""}\n\n${text}`
      : text;
    let serverThreadId = thread?.server_id ?? null;
    if (serverThreadId === null) {
      try {
        const agentName = selected.map((a) => a.name).join(", ") || "chat";
        const row = await create_chat_thread(agentName, text.slice(0, THREAD_TITLE_LEN) || "image", project_id);
        serverThreadId = row.id;
        setThreads((ts) =>
          ts.map((t) =>
            t.id === tid ? { ...t, server_id: row.id, updated_at: Date.now() } : t
          )
        );
      } catch {
        // transcript stays client-only if the thread row can't be created
      }
    }
    const outcomes = await Promise.allSettled(
      selected.map((a, i) => {
        // stream deltas into the pending bubble; a new inference round (tool
        // loop turn) resets it; tool events stream into the live tool trace
        let streamed = "";
        let liveTools: ChatToolUse[] = [];
        const run_id = make_run_id();
        myRunIds.push(run_id);
        runIdsRef.current[tid] = [...(runIdsRef.current[tid] ?? []), run_id];
        return send_agent(a, contexted, serverThreadId, image ?? undefined, (ev: ChatStreamEvent) => {
          if (ev.type === "turn") {
            streamed = "";
            patch_msg(replyIds[i], { text: "" });
          } else if (ev.type === "delta") {
            streamed += ev.text;
            patch_msg(replyIds[i], { text: streamed });
          } else if (ev.type === "tool") {
            liveTools = [
              ...liveTools,
              { tool: ev.tool, input: ev.input, ok: ev.ok, summary: ev.summary },
            ];
            patch_msg(replyIds[i], { tools: liveTools });
          }
        }, run_id).catch((e) => {
          // interrupted runs keep the partial text instead of a failed bubble
          if (String(e?.message ?? e) !== "interrupted") throw e;
          return {
            reply: `${streamed}\n\n(interrupted)`.trim(),
          } as ChatReply;
        });
      })
    );
    let failures = 0;
    outcomes.forEach((out, i) => {
      const patch = (over: Partial<Msg>) =>
        patch_thread(tid, (m) =>
          m.map((msg) => (msg.id === replyIds[i] ? { ...msg, ...over, pending: false } : msg))
        );
      if (out.status === "rejected" || out.value.reply === undefined) {
        failures += 1;
        patch({
          failed: true,
          error:
            out.status === "rejected"
              ? describe_error(out.reason)
              : "agent returned an empty reply",
        });
        return;
      }
      const data = out.value;
      const { thinking, reply } = split_thinking(data.reply as string);
      const created = data.tools
        ?.map((t) => t.summary?.match(CARD_CREATED_RE))
        .find((m) => m);
      patch({
        text: reply,
        thinking,
        card: created
          ? { id: Number(created[1]), project_id: Number(created[2]), title: created[3] }
          : undefined,
        model: data.model,
        prompt_tps: data.prompt_tps,
        tps: data.decode_tps,
        tools: data.tools,
        memories: data.memories,
      });
    });
    if (failures) {
      setError(`${failures} of ${selected.length} agent runs failed`);
    } else {
      setToast(
        selected.length === 1
          ? `agent ${selected[0].name} finished`
          : `${selected.length} agents finished`
      );
    }
    } catch (e) {
      // never leave bubbles stuck on "generating…": mark every reply of this
      // run failed and surface the error
      patch_thread(tid, (m) =>
        m.map((msg) =>
          replyIds.includes(msg.id) ? { ...msg, pending: false, failed: true, error: describe_error(e) } : msg
        )
      );
      setError(String((e as Error)?.message ?? e));
    } finally {
      const rest = (runIdsRef.current[tid] ?? []).filter((id) => !myRunIds.includes(id));
      if (rest.length) runIdsRef.current[tid] = rest;
      else delete runIdsRef.current[tid];
      bump_busy(tid, -1);
      // the queue flushes on its own once the thread goes idle (Zed-style:
      // queued messages send themselves when the reply finishes)
      const next = queuedRef.current[tid];
      if (next && !busyCount.current.has(tid)) {
        clear_queued(tid);
        void dispatch(next.text, next.images, tid);
      }
      // run ended while another thread was on screen: mark it so the user sees
      // the completion instead of finding out only after switching back
      if (tid !== activeIdRef.current) {
        setThreads((ts) => ts.map((t) => (t.id === tid ? { ...t, unread: true } : t)));
      }
    }
  }

  // stop the active run(s) on this thread: the server flags each run
  // cancelled, the stream ends and the partial text stays in the bubble
  async function stop_thread(tid: number) {
    const ids = runIdsRef.current[tid] ?? [];
    if (!ids.length) return;
    await Promise.all(ids.map((run_id) => cancel_chat_run(run_id)));
    setToast("interrupting…");
  }

  const activeIdRef = useRef(activeId);
  activeIdRef.current = activeId;
  use_card_created((card) => {
    patch_thread(activeIdRef.current, (m) => [
      ...m,
      { id: nextId.current++, role: "assistant", text: "", card },
    ]);
    setToast(`card #${card.id} created — click it in chat to open`);
  });

  const sub =
    selected.length === 0
      ? "no agent"
      : selected
          .map((a) => `${a.name}${a.model ? ` · ${pretty_name(a.model)}` : ""}`)
          .join(", ");

  const [reviewOpen, setReviewOpen] = useState(false);

  return (
    <Modal
      open={open}
      wide
      docked={docked}
      on_close={on_close}
      title={
        <>
          <img className="title-icon" src="/susutaku_jibi.png" alt="" />
          susutaku
          <span className="sub">{sub}</span>
          <button
            type="button"
            className="title-circle-btn"
            onClick={() => setReviewOpen(true)}
            title="review agent work (diff · commit · push · pr)"
            aria-label="review agent work"
            disabled={selected.length === 0}
          >
            <CircleDot size={14} />
          </button>
          <button
            type="button"
            onClick={toggle_dock}
            title={docked ? "dock centered" : "dock right"}
            aria-label={docked ? "dock centered" : "dock right"}
            aria-pressed={docked}
          >
            <PanelRight size={14} />
          </button>
        </>
      }
    >
      <div className="chat-modal-body">
        {docked && (
          <div
            className="chat-dock-resize"
            onMouseDown={start_resize}
            role="separator"
            aria-orientation="vertical"
            aria-label="resize chat panel"
          />
        )}
        <div className={`chat-main${messages.length === 0 ? " empty" : ""}`}>
          {messages.length > 0 && (
          <section
            className="log chat-modal-log"
            aria-live="polite"
            ref={logRef}
            onScroll={on_log_scroll}
          >
            {messages.map((m) => (
              <div key={m.id} className={`bubble ${m.role}`}>
                {m.agent_name && (
                  <small className="agent-tag">
                    {m.agent_name}
                    {m.model ? ` · ${pretty_name(m.model)}` : ""}
                    {machineByAgent[m.agent_name] && ` · ${machineByAgent[m.agent_name]}`}
                  </small>
                )}
                {m.thinking && (
                  <details className="thinking">
                    <summary><Brain size={12} /> thinking</summary>
                    <pre>{m.thinking}</pre>
                  </details>
                )}
                {m.tools && m.tools.length > 0 && (
                  <details className="thinking tool-trace">
                    <summary><Wrench size={12} /> tools ({m.tools.length})</summary>
                    <ul>
                      {m.tools.map((t, i) => (
                        <li key={i}>
                          <code>{t.tool}</code>{t.input ? ` ${t.input}` : ""} {t.ok === false ? "✗" : "✓"}
                          {t.summary && <pre>{t.summary}</pre>}
                          {t.artifacts && t.artifacts.length > 0 && (
                            <div className="tool-artifacts">
                              {t.artifacts.map((a) => (
                                <code key={a.path} title={`${a.kind}: ${a.path}`}>
                                  {a.kind}: {a.path}
                                </code>
                              ))}
                            </div>
                          )}
                        </li>
                      ))}
                    </ul>
                  </details>
                )}
                {m.memories && m.memories.length > 0 && (
                  <details className="thinking memory-trace">
                    <summary><Brain size={12} /> memory ({m.memories.length})</summary>
                    <ul>
                      {m.memories.map((mem, i) => (
                        <li key={i}><pre>{mem}</pre></li>
                      ))}
                    </ul>
                  </details>
                )}
                {m.image && (
                  <img
                    className="bubble-image"
                    src={m.image}
                    alt="attached screenshot"
                    onClick={() => window.open(m.image, "_blank")}
                  />
                )}
                {m.pending ? (
                  <span className="run-dots" role="status" aria-label="agent is running">
                    <span /><span /><span />
                  </span>
                ) : m.failed ? (
                  <p className="error">{m.error ?? "run failed"}</p>
                ) : (
                  <>
                    {m.text ? <MessageText text={m.text} /> : null}
                    {m.card && <CardChip card={m.card} />}
                  </>
                )}
                {!m.pending && !m.failed && !!m.tps && m.tps > 0 && (
                  <small>{m.model} · prompt {m.prompt_tps?.toFixed(1)} tok/s · decode {m.tps.toFixed(1)} tok/s</small>
                )}
              </div>
            ))}
          </section>
          )}
          {error && <p className="error">{error}</p>}
          <form className="chat-composer" onSubmit={send}>
            {queued[activeId] && (
              <div className="chat-queued" role="status">
                <span className="chat-queued-text">queued: {queued[activeId]!.text || "image"}</span>
                <button
                  type="button"
                  className="chat-queued-send"
                  onClick={() => send_now(activeId)}
                  title="interrupt the current reply and send this now"
                >
                  Send Now
                </button>
                <button
                  type="button"
                  className="chat-queued-drop"
                  onClick={() => clear_queued(activeId)}
                  aria-label="discard queued message"
                >
                  <X size={12} />
                </button>
              </div>
            )}
            {pendingImages.length > 0 && (
              <div className="chat-composer-attachments">
                {pendingImages.map((img, i) => (
                  <span className="chat-attach-chip" key={`${i}-${img.slice(-16)}`}>
                    <img src={img} alt="attached screenshot preview" />
                    <button
                      type="button"
                      onClick={() => setPendingImages((cur) => cur.filter((_, j) => j !== i))}
                      title="remove attachment"
                      aria-label={`remove attachment ${i + 1}`}
                    >
                      <X size={12} />
                    </button>
                  </span>
                ))}
              </div>
            )}
            <textarea
              ref={inputRef}
              className={`chat-input${inputDrag ? " drag" : ""}`}
              rows={6}
              value={input}
              onChange={(e) => {
                setInput(e.target.value);
                write_draft(activeKey, e.target.value);
                const el = e.target as HTMLTextAreaElement;
                el.style.height = "auto";
                el.style.height = `${Math.min(el.scrollHeight, 480)}px`;
              }}
              onKeyDown={(e: React.KeyboardEvent<HTMLTextAreaElement>) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  void send();
                }
              }}
              onPaste={(e: React.ClipboardEvent<HTMLTextAreaElement>) => {
                const files = e.clipboardData?.files;
                if (!files?.length) return;
                e.preventDefault();
                void image_files_to_data_urls(files)
                  .then((urls) => {
                    if (urls.length) {
                      setPendingImages((cur) => [...cur, ...urls]);
                      setToast("pasted image attached — it goes out with your next message");
                    }
                  })
                  .catch(() => setError("could not load pasted image"));
              }}
              onDragOver={(e: React.DragEvent<HTMLTextAreaElement>) => {
                if (e.dataTransfer.types.includes(CARD_MIME) || e.dataTransfer.types.includes("Files")) {
                  e.preventDefault();
                  setInputDrag(true);
                }
              }}
              onDragLeave={() => setInputDrag(false)}
              onDrop={(e: React.DragEvent<HTMLTextAreaElement>) => {
                setInputDrag(false);
                const id = e.dataTransfer.getData(CARD_MIME);
                if (id) {
                  e.preventDefault();
                  setInput((cur) => `${cur}${cur && !cur.endsWith(" ") ? " " : ""}#card:${id} `);
                  return;
                }
                // image files dragged from the desktop attach straight to the
                // next message; annotate via the camera button if needed
                const files = e.dataTransfer.files;
                if (files?.length) {
                  e.preventDefault();
                  void image_files_to_data_urls(files)
                    .then((urls) => {
                      if (!urls.length) return;
                      setPendingImages((cur) => [...cur, ...urls]);
                      setToast("image attached — it goes out with your next message");
                    })
                    .catch(() => setError("could not load dropped image"));
                }
              }}
              placeholder={
                messages.length === 0
                  ? "Message the agent — ⏎ to send, drop / paste an image to attach"
                  : threadBusy
                    ? "generating — ⏎ queues your message"
                    : "Message the agent — drop or paste an image, or drop a card"
              }
            />
            <div className="chat-composer-bar">
              <div className="chat-composer-tools">
                <div className="chat-agent-pick" ref={pickerRef}>
                  <button
                    type="button"
                    className="chat-agent-pick-btn"
                    onClick={() => setPickerOpen((v) => !v)}
                    title="choose agent(s)"
                    aria-label="choose agents"
                    aria-haspopup="menu"
                    aria-expanded={pickerOpen}
                  >
                    <Bot size={16} />
                    {selected.length > 0 && (
                      <span className="chat-agent-count" aria-hidden="true">
                        {selected.length}
                      </span>
                    )}
                  </button>
                  {pickerOpen && (
                    <div className="chat-agent-menu" role="menu" aria-label="agents">
                      {agents.length === 0 && <span className="dock-ws-empty">no agents yet</span>}
                      {agents.map((a) => (
                        <button
                          type="button"
                          key={a.id}
                          role="menuitemcheckbox"
                          aria-checked={selectedIds.includes(a.id as number)}
                          className={selectedIds.includes(a.id as number) ? "on" : ""}
                          onClick={() => toggle_agent(a.id as number)}
                        >
                          <span className="chat-agent-check" aria-hidden="true">
                            {selectedIds.includes(a.id as number) ? "✓" : ""}
                          </span>
                          {a.name}{a.model ? ` · ${pretty_name(a.model)}` : ""}
                          {machineByAgent[a.name] && ` · ${machineByAgent[a.name]}`}
                        </button>
                      ))}
                    </div>
                  )}
                </div>
                <select
                  className="search-toggle"
                  value={searchMode}
                  onChange={(e) => setSearchMode(e.target.value as "off" | "auto" | "on")}
                  title="web search mode"
                  aria-label="web search mode"
                >
                  <option value="off">search: off</option>
                  <option value="auto">search: auto</option>
                  <option value="on">search: on</option>
                </select>
                <button
                  type="button"
                  className={focusOn ? "chat-capscreen on" : "chat-capscreen"}
                  onClick={() => setFocusOn((f) => !f)}
                  title={focusOn ? "focus on: page context is sent — click to stop" : "focus off: send bare messages — click to include page context"}
                  aria-label="toggle page focus context"
                  aria-pressed={focusOn}
                >
                  <Crosshair size={16} />
                </button>
                <button
                  type="button"
                  className={pendingImages.length ? "chat-capscreen on" : "chat-capscreen"}
                  onClick={() => setCapscreenOpen(true)}
                  title="attach annotated screenshot"
                  aria-label="attach annotated screenshot"
                >
                  <Camera size={16} />
                </button>
              </div>
              <button
                type="submit"
                className="chat-send"
                disabled={!selected.length || (!input.trim() && !pendingImages.length)}
                title={
                  threadBusy
                    ? "queue this message — it sends when the reply finishes"
                    : "send"
                }
                aria-label={threadBusy ? "queue message" : "send message"}
              >
                <Send size={16} />
              </button>
            </div>
          </form>
          <CapscreenModal
            open={capscreenOpen}
            on_close={() => setCapscreenOpen(false)}
            on_send={async (image, note) => {
              setPendingImages((cur) => [...cur, image]);
              if (note) setInput((cur) => `${cur}${cur && !cur.endsWith(" ") ? " " : ""}${note} `);
              setToast("screenshot attached — it goes out with your next message");
            }}
          />
        </div>
        <aside className="chat-threads" aria-label="chat threads">
          <div className="chat-threads-head">
            <span>threads</span>
            <button
              type="button"
              onClick={new_thread}
              title="new thread"
              aria-label="new thread"
            >
              <MessageSquarePlus size={14} />
            </button>
          </div>
          {sortedThreads.map((t) => (
            <div key={t.id} className={`chat-thread-row ${t.id === activeId ? "active" : ""}`}>
              <button
                type="button"
                className="chat-thread-btn"
                onClick={() => {
                  setActiveId(t.id);
                  setThreads((ts) => ts.map((x) => (x.id === t.id ? { ...x, unread: false } : x)));
                }}
                title={t.title}
              >
                {t.unread ? "● " : ""}
                {t.title}
              </button>
              {t.server_id !== null && (
                <button
                  type="button"
                  className="chat-thread-share"
                  onClick={() => copy_share(t)}
                  title="copy share link"
                  aria-label={`share ${t.title}`}
                >
                  <Link2 size={12} />
                </button>
              )}
              <button
                type="button"
                className="chat-thread-close"
                onClick={() => close_thread(t.id)}
                title="close thread"
                aria-label={`close ${t.title}`}
              >
                <X size={12} />
              </button>
            </div>
          ))}
        </aside>
      </div>
      <AgentReview
        open={reviewOpen}
        agent={selected[0]?.name ?? null}
        project_id={project_id ?? undefined}
        on_close={() => setReviewOpen(false)}
      />
      {toast && (
        <p className="toast run-toast" role="status">
          {toast}
        </p>
      )}
    </Modal>
  );
}
