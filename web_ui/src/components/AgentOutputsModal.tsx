import { useEffect, useState } from "react";
import { Check, FileDiff, X } from "lucide-react";
import { fetch_agent_outputs, set_agent_output_status, type AgentOutput } from "../lib.js";
import { Modal, SlideOver } from "../ui/Overlay.js";

const STATUS_FILTERS = ["all", "pending", "approved", "rejected"] as const;

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

export function AgentOutputsModal({ open, on_close }: { open: boolean; on_close: () => void }) {
  const [outputs, set_outputs] = useState<AgentOutput[] | null>(null);
  const [error, set_error] = useState("");
  const [filter, set_filter] = useState<(typeof STATUS_FILTERS)[number]>("all");
  const [selected, set_selected] = useState<AgentOutput | null>(null);

  useEffect(() => {
    if (!open) return;
    set_outputs(null);
    set_error("");
    fetch_agent_outputs()
      .then(set_outputs)
      .catch((err: unknown) => set_error(err instanceof Error ? err.message : String(err)));
  }, [open]);

  async function decide(id: number, status: AgentOutput["status"]) {
    try {
      await set_agent_output_status(id, status);
      set_outputs((prev) =>
        (prev ?? []).map((o) => (o.id === id ? { ...o, status } : o)),
      );
      set_selected((prev) => (prev && prev.id === id ? { ...prev, status } : prev));
    } catch (err: unknown) {
      set_error(err instanceof Error ? err.message : String(err));
    }
  }

  const visible = (outputs ?? []).filter((o) => filter === "all" || o.status === filter);

  return (
    <>
      <Modal open={open} title="agent outputs" on_close={on_close} wide>
        {error && <p className="error">{error}</p>}
        {!error && !outputs && <p className="empty">loading…</p>}
        {outputs && (
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
              <ul className="output-list">
                {visible.map((o) => (
                  <li key={o.id}>
                    <button className="output-row" onClick={() => set_selected(o)}>
                      <span className="output-id">#{o.id}</span>
                      <span className="output-agent">{o.agent}</span>
                      <span className="output-patch-size">{o.patch ? `${o.patch.split("\n").length} lines` : "no diff"}</span>
                      <span className={status_badge_class(o.status)}>{o.status}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </Modal>
      <OutputDetail output={selected} on_close={() => set_selected(null)} on_decide={decide} />
    </>
  );
}
