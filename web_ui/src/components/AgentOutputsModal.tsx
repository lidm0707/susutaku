import { useEffect, useRef, useState } from "react";
import { Check, FileDiff, Play, Square, X } from "lucide-react";
import {
  fetch_agent_logs,
  fetch_agent_outputs,
  fetch_manager_agents,
  finish_manager_agent,
  set_agent_output_status,
  type AgentLogs,
  type AgentOutput,
  type ManagerAgent,
  type StoredOutcome,
} from "../lib.js";
import { Modal, SlideOver } from "../ui/Overlay.js";
import { toast } from "../ui/Toast.js";

const STATUS_FILTERS = ["all", "pending", "approved", "rejected"] as const;
const RUNNING_POLL_MS = 5000;
const STREAM_POLL_MS = 2000;

function status_badge_class(status: AgentOutput["status"]): string {
  if (status === "approved") return "output-badge approved";
  if (status === "rejected") return "output-badge rejected";
  return "output-badge pending";
}

function OutputDetail({
  output,
  on_close,
  on_decide,
}: {
  output: AgentOutput | null;
  on_close: () => void;
  on_decide: (id: number, status: AgentOutput["status"]) => void;
}) {
  return (
    <SlideOver open={output != null} title={`output #${output?.id ?? ""} — ${output?.agent ?? ""}`} on_close={on_close}>
      {output && (
        <article className="output-detail">
          <header className="output-detail-head">
            <span className={status_badge_class(output.status)}>{output.status}</span>
            <time>{new Date(output.created_at).toLocaleString()}</time>
          </header>
          {output.result && (
            <section aria-label="result">
              <h3>result</h3>
              <pre className="output-pre">{output.result}</pre>
            </section>
          )}
          {output.commit_oid && (
            <p className="output-commit">
              commit <code>{output.commit_oid.slice(0, 10)}</code>
            </p>
          )}
          <section aria-label="patch">
            <h3>
              <FileDiff size={14} /> patch
            </h3>
            {output.patch ? (
              <pre className="output-pre diff">{output.patch}</pre>
            ) : (
              <p className="empty">no changes</p>
            )}
          </section>
          {output.transcript && (
            <details className="output-transcript">
              <summary>transcript</summary>
              <pre className="output-pre">{output.transcript}</pre>
            </details>
          )}
          {output.status === "pending" && (
            <footer className="output-actions">
              <button className="danger" onClick={() => on_decide(output.id, "rejected")}>
                <X size={14} /> reject
              </button>
              <button className="primary" onClick={() => on_decide(output.id, "approved")}>
                <Check size={14} /> approve
              </button>
            </footer>
          )}
        </article>
      )}
    </SlideOver>
  );
}

function StreamPanel({ agent, on_stop }: { agent: string; on_stop: () => void }) {
  const [logs, set_logs] = useState<AgentLogs | null>(null);
  const [error, set_error] = useState("");
  const pre_ref = useRef<HTMLPreElement>(null);

  useEffect(() => {
    set_logs(null);
    set_error("");
    const tick = () =>
      fetch_agent_logs(agent)
        .then((l) => {
          set_logs(l);
          set_error("");
        })
        .catch((err: unknown) => set_error(err instanceof Error ? err.message : String(err)));
    tick();
    const timer = setInterval(tick, STREAM_POLL_MS);
    return () => clearInterval(timer);
  }, [agent]);

  useEffect(() => {
    const pre = pre_ref.current;
    if (pre) pre.scrollTop = pre.scrollHeight;
  }, [logs]);

  return (
    <section className="agent-stream" aria-label={`live stream of ${agent}`}>
      <header className="agent-stream-head">
        <span className="agent-stream-dot" aria-hidden="true" />
        <span className="agent-logs-label">live — {agent}</span>
        {logs && <span className="agent-logs-label">{logs.runs} runs</span>}
        <button type="button" className="agent-stream-stop" onClick={on_stop} aria-label="stop streaming">
          <Square size={12} />
        </button>
      </header>
      {error && <p className="error">{error}</p>}
      {logs?.last_result && <p className="agent-logs-result">last result: {logs.last_result}</p>}
      {logs && logs.transcript.length === 0 && <p className="empty">no transcript yet</p>}
      {logs && logs.transcript.length > 0 && (
        <pre className="agent-logs-pre" ref={pre_ref}>
          {logs.transcript.join("\n")}
        </pre>
      )}
    </section>
  );
}

function RunningList({
  runs,
  stream_agent,
  on_stream,
  on_finish,
  finishing,
}: {
  runs: ManagerAgent[];
  stream_agent: string | null;
  on_stream: (agent: string) => void;
  on_finish: (agent: string) => void;
  finishing: string | null;
}) {
  if (runs.length === 0) return <p className="empty">no agents running</p>;
  return (
    <ul className="agent-run-list">
      {runs.map((r) => (
        <li key={r.agent} className={`agent-run-row ${stream_agent === r.agent ? "active" : ""}`}>
          <button className="agent-run-main" onClick={() => on_stream(r.agent)} aria-label={`stream ${r.agent}`}>
            <Play size={12} />
            <span className="output-agent">{r.agent}</span>
            <span className="agent-run-meta">{r.runs} runs</span>
            {r.last_cmd && <code className="agent-run-cmd" title={r.last_cmd}>{r.last_cmd}</code>}
          </button>
          <button
            className="agent-run-finish"
            disabled={finishing === r.agent}
            onClick={() => on_finish(r.agent)}
          >
            {finishing === r.agent ? "finishing…" : "finish"}
          </button>
        </li>
      ))}
    </ul>
  );
}

export function AgentOutputsModal({ open, on_close }: { open: boolean; on_close: () => void }) {
  const [runs, set_runs] = useState<ManagerAgent[] | null>(null);
  const [stream_agent, set_stream_agent] = useState<string | null>(null);
  const [finishing, set_finishing] = useState<string | null>(null);
  const [outputs, set_outputs] = useState<AgentOutput[] | null>(null);
  const [error, set_error] = useState("");
  const [filter, set_filter] = useState<(typeof STATUS_FILTERS)[number]>("all");
  const [selected, set_selected] = useState<AgentOutput | null>(null);

  useEffect(() => {
    if (!open) return;
    set_error("");
    const tick = () =>
      fetch_manager_agents()
        .then(set_runs)
        .catch((err: unknown) => set_error(err instanceof Error ? err.message : String(err)));
    tick();
    const timer = setInterval(tick, RUNNING_POLL_MS);
    return () => clearInterval(timer);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    set_outputs(null);
    fetch_agent_outputs()
      .then(set_outputs)
      .catch((err: unknown) => set_error(err instanceof Error ? err.message : String(err)));
  }, [open]);

  async function finish(agent: string) {
    set_finishing(agent);
    try {
      const outcome: StoredOutcome = await finish_manager_agent(agent);
      if (stream_agent === agent) set_stream_agent(null);
      toast(`agent ${agent} finished — output #${outcome.output_id} stored`);
      const fresh = await fetch_agent_outputs();
      set_outputs(fresh);
      const stored = fresh.find((o) => o.id === outcome.output_id);
      if (stored) set_selected(stored);
    } catch (err: unknown) {
      toast(err instanceof Error ? err.message : String(err));
    } finally {
      set_finishing(null);
    }
  }

  async function decide(id: number, status: AgentOutput["status"]) {
    try {
      await set_agent_output_status(id, status);
      set_outputs((prev) =>
        (prev ?? []).map((o) => (o.id === id ? { ...o, status } : o)),
      );
      set_selected((prev) => (prev && prev.id === id ? { ...prev, status } : prev));
    } catch (err: unknown) {
      toast(err instanceof Error ? err.message : String(err));
    }
  }

  const visible = (outputs ?? []).filter((o) => filter === "all" || o.status === filter);

  return (
    <>
      <Modal open={open} title="agents" on_close={on_close} wide>
        {error && <p className="error">{error}</p>}
        <section aria-label="running agents">
          <h3>running</h3>
          {!runs ? <p className="empty">loading…</p> : (
            <RunningList
              runs={runs}
              stream_agent={stream_agent}
              on_stream={(agent) => set_stream_agent((cur) => (cur === agent ? null : agent))}
              on_finish={finish}
              finishing={finishing}
            />
          )}
          {stream_agent && <StreamPanel agent={stream_agent} on_stop={() => set_stream_agent(null)} />}
        </section>
        <section aria-label="finished outputs">
          <h3>outputs</h3>
          {!outputs ? (
            <p className="empty">loading…</p>
          ) : (
            <div className="agent-outputs">
              <div role="radiogroup" aria-label="status filter" className="output-filters">
                {STATUS_FILTERS.map((f) => (
                  <button
                    key={f}
                    role="radio"
                    aria-checked={filter === f}
                    className={filter === f ? "active" : ""}
                    onClick={() => set_filter(f)}
                  >
                    {f}
                  </button>
                ))}
              </div>
              {visible.length === 0 ? (
                <p className="empty">no outputs yet — finish an agent task first</p>
              ) : (
                visible.map((o) => (
                  <button key={o.id} className="output-row" onClick={() => set_selected(o)}>
                    <span className="output-id">#{o.id}</span>
                    <span className="output-agent">{o.agent}</span>
                    <span className="output-patch-size">{o.patch ? `${o.patch.split("\n").length} lines` : "no diff"}</span>
                    <span className={status_badge_class(o.status)}>{o.status}</span>
                  </button>
                ))
              )}
            </div>
          )}
        </section>
      </Modal>
      <OutputDetail output={selected} on_close={() => set_selected(null)} on_decide={decide} />
    </>
  );
}
