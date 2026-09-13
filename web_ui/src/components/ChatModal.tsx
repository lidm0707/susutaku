import { useEffect, useRef, useState } from "react";
import { useLocation } from "react-router-dom";
import { Bot, Brain, Camera, Check, Copy, Link2, MessageSquarePlus, PanelRight, Send, Wrench, X } from "lucide-react";
import {
  API_BASE,
  chat_codex,
  chat_zai,
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
  pretty_name,
  select_model,
  type Agent,
  type ChatReply,
  type ChatToolUse,
  type CodexModel,
  type CronJob,
  type ModelInfo,
  type PromptSection,
} from "../lib.js";
import { Modal } from "../ui/Overlay.js";
import CapscreenModal from "./CapscreenModal.js";
import { image_file_to_canvas } from "../features/capscreen.js";

const MAX_TOKENS = 512;

const THINK_OPEN = "<think>";
const THINK_CLOSE = "</think>";

const TOAST_MS = 4000;

const THREAD_TITLE_LEN = 24;

const CARD_MIME = "application/x-susutaku-card";
const CARD_MENTION_RE = /#card:(\d+)/g;

const DOCK_KEY = "chat_dock";

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
  "/kanban": "kanban",
  "/pipelines": "pipelines",
  "/routine": "routine",
  "/agents": "agents",
  "/settings": "settings",
  "/sandbox": "sandbox",
  "/": "login",
};

export function page_label(pathname: string): string {
  return PAGE_LABELS[pathname] ?? "kanban";
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
      (j.pipeline_name ? `, pipeline "${j.pipeline_name}"` : "") +
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

function MessageText({ text }: { text: string }) {
  const segments = split_segments(text);
  return (
    <div className="msg-body">
      {segments.map((s, i) =>
        s.kind === "code" ? <CodeBlock key={i} lang={s.lang} body={s.body} /> : <InlineText key={i} body={s.body} />,
      )}
    </div>
  );
}

interface Msg {
  id: number;
  role: "user" | "assistant";
  text: string;
  pending?: boolean;
  failed?: boolean;
  thinking?: string;
  agent_name?: string;
  model?: string;
  prompt_tps?: number;
  tps?: number;
  /// annotated screenshot attached to this message
  image?: string;
  tools?: ChatToolUse[];
  memories?: string[];
}

interface Thread {
  id: number;
  title: string;
  messages: Msg[];
  server_id: number | null;
  loaded: boolean;
}

export default function ChatModal({ open, on_close }: { open: boolean; on_close: () => void }) {
  const { pathname } = useLocation();
  const [threads, setThreads] = useState<Thread[]>([
    { id: 0, title: "thread 1", messages: [], server_id: null, loaded: true },
  ]);
  const [activeId, setActiveId] = useState(0);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
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
  // server thread id taken from a shared ?chat= link, resolved once threads load
  const [sharedId, setSharedId] = useState(
    () => Number(new URLSearchParams(window.location.search).get(CHAT_PARAM)) || 0,
  );
  // agent name -> machine it currently runs on ("" = not running anywhere).
  const [machineByAgent, setMachineByAgent] = useState<Record<string, string>>({});
  const [capscreenOpen, setCapscreenOpen] = useState(false);
  const [pendingImage, setPendingImage] = useState<string | null>(null);
  const [capscreenInit, setCapscreenInit] = useState<HTMLCanvasElement | null>(null);
  const nextId = useRef(1);
  const nextThreadId = useRef(1);
  const loadingThreads = useRef(new Set<number>());
  const pickerRef = useRef<HTMLDivElement | null>(null);
  const active = threads.find((t) => t.id === activeId) ?? threads[0];
  const messages = active.messages;
  const selected = agents.filter((a) => selectedIds.includes(a.id as number));

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
    fetch_chat_threads()
      .then((rows) =>
        setThreads((ts) => {
          const locals = ts.filter((t) => t.server_id === null);
          const nextLocalId = Math.max(0, ...locals.map((t) => t.id)) + 1;
          nextThreadId.current = Math.max(nextThreadId.current, nextLocalId);
          const server: Thread[] = rows.map((r) => ({
            id: nextThreadId.current++,
            title: r.title || `thread ${r.id}`,
            messages: [],
            server_id: r.id,
            loaded: false,
          }));
          return [...server, ...locals];
        })
      )
      .catch(() => {});
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
    window.history.replaceState(null, "", window.location.pathname);
  }, [sharedId, threads]);

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

  function patch_thread(id: number, fn: (msgs: Msg[]) => Msg[]) {
    setThreads((ts) => ts.map((t) => (t.id === id ? { ...t, messages: fn(t.messages) } : t)));
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
      { id, title: `thread ${id + 1}`, messages: [], server_id: null, loaded: true },
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
    image?: string
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
      const res = await fetch(`${API_BASE}/api/chat`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ message: text, max_tokens: MAX_TOKENS, search: searchMode, agent: a.name }),
      });
      return res.json();
    }
    // agents flagged receive_images=false get text only — the backend rejects
    // images for them with 403
    const img = a.receive_images === false ? undefined : image;
    return chat_zai(text, a.model, agent_sections(a), a.name, thread_id ?? undefined, img);
  }

  async function send(e: React.FormEvent) {
    e.preventDefault();
    const text = input.trim();
    if (!text || busy) return;
    if (!selected.length) {
      setError("no agent — pick one with the + button");
      return;
    }
    const tid = activeId;
    const thread = threads.find((t) => t.id === tid);
    setInput("");
    setError("");
    setBusy(true);
    set_title_from(tid, text);
    const userId = nextId.current++;
    const replyIds = selected.map(() => nextId.current++);
    const attachedImage = pendingImage;
    setPendingImage(null);
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
    const page = page_label(pathname);
    const data = await page_data(pathname);
    const cards = await card_context(text);
    const image = attachedImage;
    const contexted =
      `[context: user is currently on the ${page} page${data ? `\n${data}` : ""}${cards ? `\n${cards}` : ""}]${image ? "\n[a screenshot is attached]" : ""}\n\n${text}`;
    let serverThreadId = thread?.server_id ?? null;
    if (serverThreadId === null) {
      try {
        const agentName = selected.map((a) => a.name).join(", ") || "chat";
        const row = await create_chat_thread(agentName, text.slice(0, THREAD_TITLE_LEN));
        serverThreadId = row.id;
        setThreads((ts) => ts.map((t) => (t.id === tid ? { ...t, server_id: row.id } : t)));
      } catch {
        // transcript stays client-only if the thread row can't be created
      }
    }
    const outcomes = await Promise.allSettled(
      selected.map((a) => send_agent(a, contexted, serverThreadId, image ?? undefined))
    );
    let failures = 0;
    outcomes.forEach((out, i) => {
      const patch = (over: Partial<Msg>) =>
        patch_thread(tid, (m) =>
          m.map((msg) => (msg.id === replyIds[i] ? { ...msg, ...over, pending: false } : msg))
        );
      if (out.status === "rejected" || out.value.reply === undefined) {
        failures += 1;
        patch({ failed: true });
        return;
      }
      const data = out.value;
      const { thinking, reply } = split_thinking(data.reply as string);
      patch({
        text: reply,
        thinking,
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
    setBusy(false);
  }

  const sub =
    selected.length === 0
      ? "no agent"
      : selected
          .map((a) => `${a.name}${a.model ? ` · ${pretty_name(a.model)}` : ""}`)
          .join(", ");

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
        <div className="chat-main">
          <section className="log chat-modal-log" aria-live="polite">
            {messages.length === 0 && <p className="empty">Say something to the agent.</p>}
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
                  <p className="error">run failed</p>
                ) : (
                  <MessageText text={m.text} />
                )}
                {!m.pending && !m.failed && !!m.tps && m.tps > 0 && (
                  <small>{m.model} · prompt {m.prompt_tps?.toFixed(1)} tok/s · decode {m.tps.toFixed(1)} tok/s</small>
                )}
              </div>
            ))}
          </section>
          {error && <p className="error">{error}</p>}
          <form onSubmit={send}>
            <select
              className="search-toggle"
              value={searchMode}
              onChange={(e) => setSearchMode(e.target.value as "off" | "auto" | "on")}
              title="web search mode"
              aria-label="web search mode"
            >
              <option value="off">off</option>
              <option value="auto">auto</option>
              <option value="on">on</option>
            </select>
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
            {pendingImage && (
              <span className="chat-attach-chip">
                <img src={pendingImage} alt="attached screenshot preview" />
                <button
                  type="button"
                  onClick={() => setPendingImage(null)}
                  title="remove attachment"
                  aria-label="remove attachment"
                >
                  <X size={12} />
                </button>
              </span>
            )}
            <input
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onDragOver={(e: React.DragEvent<HTMLInputElement>) => {
                if (e.dataTransfer.types.includes(CARD_MIME) || e.dataTransfer.types.includes("Files"))
                  e.preventDefault();
              }}
              onDrop={(e: React.DragEvent<HTMLInputElement>) => {
                const id = e.dataTransfer.getData(CARD_MIME);
                if (id) {
                  e.preventDefault();
                  setInput((cur) => `${cur}${cur && !cur.endsWith(" ") ? " " : ""}#card:${id} `);
                  return;
                }
                // image file dragged from the desktop → open the annotator with it
                const file = e.dataTransfer.files?.[0];
                if (file && file.type.startsWith("image/")) {
                  e.preventDefault();
                  image_file_to_canvas(file)
                    .then((canvas) => {
                      setCapscreenInit(canvas);
                      setCapscreenOpen(true);
                    })
                    .catch(() => setError("could not load dropped image"));
                }
              }}
              placeholder={busy ? "generating…" : "type a message or drop a card"}
            />
            <button
              type="button"
              className={pendingImage ? "chat-capscreen on" : "chat-capscreen"}
              onClick={() => setCapscreenOpen(true)}
              title="attach annotated screenshot"
              aria-label="attach annotated screenshot"
            >
              <Camera size={16} />
            </button>
            <button type="submit" disabled={busy || !selected.length}>
              <Send size={16} />
            </button>
          </form>
          <CapscreenModal
            open={capscreenOpen}
            initial_image={capscreenInit}
            on_close={() => {
              setCapscreenOpen(false);
              setCapscreenInit(null);
            }}
            on_send={async (image, note) => {
              setPendingImage(image);
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
          {threads.map((t) => (
            <div key={t.id} className={`chat-thread-row ${t.id === activeId ? "active" : ""}`}>
              <button
                type="button"
                className="chat-thread-btn"
                onClick={() => setActiveId(t.id)}
                title={t.title}
              >
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
      {toast && (
        <p className="toast run-toast" role="status">
          {toast}
        </p>
      )}
    </Modal>
  );
}
