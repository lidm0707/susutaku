import { useEffect, useState } from "react";
import {
  fetch_manager_agents,
  finish_manager_agent,
  type ManagerAgent,
} from "../lib.js";
import { Modal } from "../ui/Overlay.js";
import { toast } from "../ui/Toast.js";

const RUNNING_POLL_MS = 5000;

function RunningList({
  runs,
  on_finish,
  finishing,
}: {
  runs: ManagerAgent[];
  on_finish: (agent: string) => void;
  finishing: string | null;
}) {
  if (runs.length === 0) return <p className="empty">no agents running</p>;
  return (
    <ul className="agent-run-list">
      {runs.map((r) => (
        <li key={r.agent} className="agent-run-row">
          <div className="agent-run-main">
            <span className="output-agent">{r.agent}</span>
            <span className="agent-run-meta">{r.runs} runs</span>
            {r.last_cmd && <code className="agent-run-cmd" title={r.last_cmd}>{r.last_cmd}</code>}
          </div>
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

export function AgentRunsModal({ open, on_close }: { open: boolean; on_close: () => void }) {
  const [runs, set_runs] = useState<ManagerAgent[] | null>(null);
  const [finishing, set_finishing] = useState<string | null>(null);
  const [error, set_error] = useState("");

  useEffect(() => {
    if (!open) return;
    set_error("");
    const load = () =>
      fetch_manager_agents()
        .then(set_runs)
        .catch((err: unknown) => set_error(err instanceof Error ? err.message : String(err)));
    load();
    const t = setInterval(load, RUNNING_POLL_MS);
    return () => clearInterval(t);
  }, [open]);

  async function finish(agent: string) {
    set_finishing(agent);
    try {
      await finish_manager_agent(agent);
      toast(`agent ${agent} finished — terminal output posted to the card`);
      set_runs((prev) => (prev ?? []).filter((r) => r.agent !== agent));
    } catch (err: unknown) {
      toast(err instanceof Error ? err.message : String(err));
    } finally {
      set_finishing(null);
    }
  }

  return (
    <Modal open={open} title="agents" on_close={on_close}>
      {error && <p className="error">{error}</p>}
      <section aria-label="running agents">
        <h3>running</h3>
        {!runs ? <p className="empty">loading…</p> : (
          <RunningList runs={runs} on_finish={finish} finishing={finishing} />
        )}
      </section>
    </Modal>
  );
}
