import { useEffect, useState } from "react";
import { useLocation } from "react-router-dom";
import { Brain, Send } from "lucide-react";
import {
  API_BASE,
  chat_codex,
  chat_zai,
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

const MAX_TOKENS = 512;

const THINK_OPEN = "<think>";
const THINK_CLOSE = "</think>";

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
  role: "user" | "assistant";
  text: string;
  pending?: boolean;
  thinking?: string;
  model?: string;
  prompt_tps?: number;
  tps?: number;
}

export default function ChatModal({ open, on_close }: { open: boolean; on_close: () => void }) {
  const { pathname } = useLocation();
  const [messages, setMessages] = useState<Msg[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [codexModels, setCodexModels] = useState<CodexModel[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [agentId, setAgentId] = useState<number | null>(null);
  const [sysPrompt, setSysPrompt] = useState("");
  const [searchMode, setSearchMode] = useState<"off" | "auto" | "on">("off");
  const [error, setError] = useState("");
  const agent = agents.find((a) => a.id === agentId) ?? null;

  useEffect(() => {
    if (!open) return;
    fetch_models().then(setModels).catch(() => {});
    fetch_codex_models().then(setCodexModels).catch(() => {});
    fetch_agents()
      .then((list: Agent[]) => {
        const real = list.filter((a) => a.id !== "new");
        setAgents(real);
        setAgentId((cur) => cur ?? (real.length ? (real[0].id as number) : null));
      })
      .catch(() => {});
    fetch_system_prompt().then(setSysPrompt).catch(() => {});
  }, [open]);

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
        body: JSON.stringify({ message: text, max_tokens: MAX_TOKENS, search: searchMode }),
      });
      return res.json();
    }
    return chat_zai(text, a.model, agent_sections(a));
  }

  async function send(e: React.FormEvent) {
    e.preventDefault();
    const text = input.trim();
    if (!text || busy) return;
    if (!agent) {
      setError("no agent — create one under agents");
      return;
    }
    setInput("");
    setError("");
    setBusy(true);
    setMessages((m) => [...m, { role: "user", text }, { role: "assistant", text: "", pending: true }]);
    const page = page_label(pathname);
    const contexted = `[context: user is currently on the ${page} page]\n\n${text}`;
    try {
      const data: ChatReply = await send_agent(agent, contexted);
      if (data.reply === undefined) throw new Error(`${data.status || ""} ${JSON.stringify(data)}`);
      const { thinking, reply } = split_thinking(data.reply);
      setMessages((m) =>
        m.map((msg, i) =>
          i === m.length - 1
            ? { role: "assistant", text: reply, thinking, model: data.model, prompt_tps: data.prompt_tps, tps: data.decode_tps }
            : msg
        )
      );
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
      setMessages((m) => m.filter((msg, i) => !(i === m.length - 1 && msg.pending)));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      open={open}
      wide
      on_close={on_close}
      title={
        <>
          <img className="title-icon" src="/susutaku_jibi.png" alt="" />
          susutaku
          <span className="sub">
            {agent
              ? `${agent.name}${agent.model ? ` · ${pretty_name(agent.model)}` : ""}`
              : "no agent"}
          </span>
        </>
      }
    >
      <div className="chat-modal-body">
        <section className="log chat-modal-log">
          {messages.length === 0 && <p className="empty">Say something to the agent.</p>}
          {messages.map((m, i) => (
            <div key={i} className={`bubble ${m.role}`}>
              {m.thinking && (
                <details className="thinking">
                  <summary><Brain size={12} /> thinking</summary>
                  <pre>{m.thinking}</pre>
                </details>
              )}
              <p>{m.text || (m.pending ? "…" : "")}</p>
              {!!m.tps && m.tps > 0 && <small>{m.model} · prompt {m.prompt_tps?.toFixed(1)} tok/s · decode {m.tps.toFixed(1)} tok/s</small>}
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
          <select
            className="search-toggle"
            value={agentId ?? ""}
            onChange={(e) => setAgentId(e.target.value === "" ? null : Number(e.target.value))}
            title="agent"
          >
            {agents.length === 0 && <option value="">no agents yet</option>}
            {agents.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}{a.model ? ` · ${pretty_name(a.model)}` : ""}
              </option>
            ))}
          </select>
          <input
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder={busy ? "generating…" : "type a message"}
          />
          <button type="submit" disabled={busy || !agent}>
            <Send size={16} />
          </button>
        </form>
      </div>
    </Modal>
  );
}
