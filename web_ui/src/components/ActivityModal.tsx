import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import {
  fetch_activity,
  fetch_card_agent,
  type ActivityEntry,
  type CardRun,
} from "../lib.js";
import { Modal } from "../ui/Overlay.js";

const POLL_TICK_MS = 5000;

const RUN_MSG_RE = /^started pipeline run for task (\d+)$/;

export function run_task_id(message: string): number | null {
  const m = RUN_MSG_RE.exec(message.trim());
  return m ? Number(m[1]) : null;
}

function RunDetail({ task_id }: { task_id: number }) {
  const [run, set_run] = useState<CardRun | null>(null);
  const [agent, set_agent] = useState<string | null>(null);
  const [error, set_error] = useState("");

  useEffect(() => {
    let live = true;
    set_run(null);
    set_error("");
    fetch_card_agent(task_id)
      .then((a) => {
        if (!live) return;
        set_agent(a.name);
        set_run(a.state?.run ?? null);
      })
      .catch((err: unknown) => {
        if (live) set_error(err instanceof Error ? err.message : String(err));
      });
    return () => {
      live = false;
    };
  }, [task_id]);

  return (
    <div className="agent-logs">
      <div className="agent-logs-meta">
        <span className="agent-logs-label">task</span>
        <code>#{task_id}</code>
        {agent && <span className="agent-logs-label">agent {agent}</span>}
      </div>
      {error && <p className="error">{error}</p>}
      {!error && run && (
        <>
          <p className="agent-logs-result">
            {run.pipeline_name} · {run.status} · finished {run.finished_at}
          </p>
          {run.stages.length === 0 ? (
            <p className="empty">no stages recorded</p>
          ) : (
            <pre className="agent-logs-pre">
              {run.stages.map((s) => `${s.node}/${s.stage}: ${s.status}${s.note ? ` — ${s.note}` : ""}`).join("\n")}
            </pre>
          )}
          <p className="agent-logs-result">result: {run.output ?? "—"}</p>
        </>
      )}
      {!error && run === null && <p className="empty">loading run…</p>}
    </div>
  );
}

function EntryDetail({ entry }: { entry: ActivityEntry }) {
  const task_id = run_task_id(entry.message);
  return (
    <div className="agent-logs">
      <div className="agent-logs-meta">
        <span className={`dot kind-${entry.kind}`} aria-hidden="true" title={entry.kind} />
        <span className="agent-logs-label">{entry.kind}</span>
        <span className="dock-activity-time">{entry.created_at}</span>
      </div>
      <p className="agent-logs-result">{entry.message}</p>
      {task_id != null && <RunDetail task_id={task_id} />}
      {task_id == null && <p className="empty">no log attached to this entry</p>}
    </div>
  );
}

export function ActivityModal({ open, on_close }: { open: boolean; on_close: () => void }) {
  const [activity, set_activity] = useState<ActivityEntry[] | null>(null);
  const [error, set_error] = useState(false);
  const [selected, set_selected] = useState<ActivityEntry | null>(null);

  async function load() {
    set_error(false);
    try {
      set_activity(await fetch_activity());
    } catch {
      set_error(true);
    }
  }

  useEffect(() => {
    if (!open) return;
    void load();
    const tick = setInterval(load, POLL_TICK_MS);
    return () => clearInterval(tick);
  }, [open]);

  return (
    <Modal open={open} title="activity" on_close={on_close} wide className="machines-modal">
      <div className="machines-layout">
        <section className="machines-register" aria-label="activity list">
          <div className="dock-machines-head">
            <span>activity</span>
            <button onClick={() => void load()} title="refresh" aria-label="refresh activity">
              <RefreshCw size={13} />
            </button>
          </div>
          <div className="dock-activity-list">
            {error && <span className="dock-ws-empty">activity unavailable</span>}
            {!error && activity === null && <span className="dock-ws-empty">loading…</span>}
            {!error && activity?.length === 0 && <span className="dock-ws-empty">no activity yet</span>}
            {activity?.map((e) => (
              <button
                key={e.id}
                className={`dock-machine-row ${selected?.id === e.id ? "selected" : ""}`}
                onClick={() => set_selected(e)}
                title={e.message}
              >
                <span className={`dot kind-${e.kind}`} aria-hidden="true" title={e.kind} />
                <span className="dock-activity-time">{new Date(e.created_at).toLocaleTimeString()}</span>
                <span className="dock-activity-msg">{e.message}</span>
              </button>
            ))}
          </div>
        </section>
        <section className="machines-logs" aria-label="activity detail">
          {selected === null && <p className="empty">select an entry…</p>}
          {selected !== null && <EntryDetail entry={selected} />}
        </section>
      </div>
    </Modal>
  );
}
