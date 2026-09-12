import { useEffect, useRef, useState } from "react";
import { useLocation } from "react-router-dom";
import { Bot, Brain, Crosshair, MessageSquarePlus, PanelRight, Send, X } from "lucide-react";
import {
  API_BASE,
  chat_codex,
  chat_zai,
  fetch_agent_machine,
  fetch_agents,
  fetch_codex_models,
  fetch_models,
  fetch_system_prompt,
  pretty_name,
  select_model,
  type Agent,
  type ChatReply,
  type CodexModel,
  type ModelInfo,
  type PromptSection,
} from "../lib.js";
import { Modal } from "../ui/Overlay.js";
import { focus_label, focus_section, set_focus, use_focus, type Focus } from "./focus.js";

const MAX_TOKENS = 512;

const THINK_OPEN = "<think>";
const THINK_CLOSE = "</think>";

const TOAST_MS = 4000;

const THREAD_TITLE_LEN = 24;

const DOCK_KEY = "chat_dock";
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
  "/cronjobs": "cronjobs",
  "/agents": "agents",
  "/settings": "settings",
  "/sandbox": "sandbox",
  "/": "login",
};

export function page_label(pathname: string): string {
  return PAGE_LABELS[pathname] ?? "kanban";
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
}

interface Thread {
  id: number;
  title: string;
  messages: Msg[];
  focus: Focus | null;
  detached: boolean;
}

export default function ChatModal({ open, on_close }: { open: boolean; on_close: () => void }) {
  const { pathname } = useLocation();
  const [threads, setThreads] = useState<Thread[]>([
    { id: 0, title: "thread 1", messages: [], focus: null, detached: false },
  ]);
  const [activeId, setActiveId] = useState(0);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [codexModels, setCodexModels] = useState<CodexModel[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [selectedIds, setSelectedIds] = useState<number[]>([]);
  const [sysPrompt, setSysPrompt] = useState("");
  const [searchMode, setSearchMode] = useState<"off" | "auto" | "on">("off");
  const [pickerOpen, setPickerOpen] = useState(false);
  const [docked, setDocked] = useState(read_dock);
  const dock_w = useRef(read_dock_w());
  const [error, setError] = useState("");
  const [toast, setToast] = useState("");
  // agent name -> machine it currently runs on ("" = not running anywhere).
  const [machineByAgent, setMachineByAgent] = useState<Record<string, string>>({});
  const nextId = useRef(1);
  const nextThreadId = useRef(1);
  const pickerRef = useRef<HTMLDivElement | null>(null);
  const active = threads.find((t) => t.id === activeId) ?? threads[0];
  const messages = active.messages;
  const selected = agents.filter((a) => selectedIds.includes(a.id as number));
  const liveFocus = use_focus();
  const threadFocus = active.detached ? null : (active.focus ?? liveFocus);

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
  }, [open]);

  useEffect(() => {
    if (!open) return;
    agents.forEach((a) => {
      if (a.name in machineByAgent) return;
      fetch_agent_machine(a.name)
        .then((w) => setMachineByAgent((m) => ({ ...m, [a.name]: w.machine })))
        .catch(() => setMachineByAgent((m) => ({ ...m, [a.name]: "" })));
    });
  }, [open, agents]);

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
      localStorage.setItem(DOCK_W_KEY, dock_w.current);
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

  function detach_focus() {
    set_focus(null);
    setThreads((ts) =>
      ts.map((t) =>
        t.id === activeId
          ? t.messages.length
            ? { ...t, detached: true }
            : t
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
      { id, title: `thread ${id + 1}`, messages: [], focus: null, detached: false },
    ]);
    setActiveId(id);
    setError("");
  }

  function close_thread(id: number) {
    setThreads((ts) => {
      const rest = ts.filter((t) => t.id !== id);
      if (!rest.length) {
        const fresh = {
          id: nextThreadId.current++,
          title: "thread 1",
          messages: [] as Msg[],
          focus: null,
          detached: false,
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

  function agent_sections(a: Agent): PromptSection[] {
    const instructions = [a.persona, a.prompt].filter((s) => s.trim()).join("\n\n");
    return [
      { role: "system", body: sysPrompt },
      { role: "instructions", body: instructions },
    ].filter((s) => s.body.trim());
  }

  async function send_agent(a: Agent, text: string): Promise<ChatReply> {
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
    return chat_zai(text, a.model, agent_sections(a), a.name);
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
    const focus: Focus | null =
      thread && thread.messages.length ? (thread.detached ? null : thread.focus) : liveFocus;
    setInput("");
    setError("");
    setBusy(true);
    set_title_from(tid, text);
    const userId = nextId.current++;
    const replyIds = selected.map(() => nextId.current++);
    patch_thread(tid, (m) => [
      ...m,
      { id: userId, role: "user", text },
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
    const focusText = focus_section(focus);
    const contexted = focusText
      ? `${focusText}\n[context: user is currently on the ${page} page]\n\n${text}`
      : `[context: user is currently on the ${page} page]\n\n${text}`;
    setThreads((ts) =>
      ts.map((t) => (t.id === tid && !t.messages.length ? { ...t, focus } : t))
    );
    const outcomes = await Promise.allSettled(selected.map((a) => send_agent(a, contexted)));
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
                {m.pending ? (
                  <span className="run-dots" role="status" aria-label="agent is running">
                    <span /><span /><span />
                  </span>
                ) : m.failed ? (
                  <p className="error">run failed</p>
                ) : (
                  <p>{m.text}</p>
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
            {threadFocus && (
              <span className="chat-focus-chip" title={focus_label(threadFocus)}>
                <Crosshair size={11} />
                <span className="chat-focus-label">{focus_label(threadFocus)}</span>
                <button
                  type="button"
                  onClick={detach_focus}
                  aria-label="detach focus"
                  title="detach focus"
                >
                  <X size={10} />
                </button>
              </span>
            )}
            <input
              value={input}
              onChange={(e) => setInput(e.target.value)}
              placeholder={busy ? "generating…" : "type a message"}
            />
            <button type="submit" disabled={busy || !selected.length}>
              <Send size={16} />
            </button>
          </form>
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
