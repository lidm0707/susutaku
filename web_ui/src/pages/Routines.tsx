import { useCallback, useEffect, useState } from "react";
import { Bot, CalendarClock, Pencil, Play, Plus, Trash2 } from "lucide-react";
import {
  create_routine,
  delete_routine,
  fetch_agents,
  fetch_routine_runs,
  fetch_routines,
  run_routine,
  update_routine,
  type Agent,
  type Routine,
  type RoutineRun,
} from "../lib.js";
import { Modal } from "../ui/Overlay.js";
import { toast } from "../ui/Toast.js";
import { use_projects } from "../components/ProjectContext.tsx";
import { cron_label, RoutineEditor } from "../components/RoutineEditor.js";

interface Draft {
  id: number | null;
  name: string;
  cron: string;
  agent: string;
  instruction: string;
  enabled: boolean;
}

const EMPTY: Draft = {
  id: null,
  name: "",
  cron: "0 9 * * *",
  agent: "",
  instruction: "",
  enabled: true,
};

export default function Routines() {
  const { project_id } = use_projects();
  const [routines, setRoutines] = useState<Routine[] | null>(null);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [runsFor, setRunsFor] = useState<Routine | null>(null);
  const [runs, setRuns] = useState<RoutineRun[] | null>(null);
  const [runningId, setRunningId] = useState<number | null>(null);

  const refresh = useCallback(async () => {
    setRoutines(await fetch_routines().catch(() => null));
  }, []);

  useEffect(() => {
    refresh();
    fetch_agents()
      .then(setAgents)
      .catch(() => {});
  }, [refresh]);

  async function save(e: React.FormEvent) {
    e.preventDefault();
    if (!draft || !draft.name.trim() || !draft.cron.trim()) return;
    try {
      if (draft.id == null) {
        await create_routine(draft.name.trim(), draft.cron.trim(), draft.agent, draft.instruction.trim(), draft.enabled);
      } else {
        await update_routine(draft.id, draft.name.trim(), draft.cron.trim(), draft.agent, draft.instruction.trim(), draft.enabled);
      }
      setDraft(null);
      await refresh();
      toast("routine saved", "success");
    } catch (err) {
      toast(err instanceof Error ? err.message : String(err));
    }
  }

  async function remove(id: number) {
    try {
      await delete_routine(id);
      setRoutines((cur) => (cur ?? []).filter((r) => r.id !== id));
      toast("routine deleted");
    } catch (err) {
      toast(err instanceof Error ? err.message : String(err));
    }
  }

  async function run_now(r: Routine) {
    setRunningId(r.id);
    try {
      const res = await run_routine(r.id);
      toast(`routine ${r.name}: ${res.ok ? "ran" : "failed"} — ${res.summary.slice(0, 80)}`);
    } catch (err) {
      toast(err instanceof Error ? err.message : String(err));
    } finally {
      setRunningId(null);
    }
  }

  async function open_runs(r: Routine) {
    setRunsFor(r);
    setRuns(null);
    try {
      setRuns(await fetch_routine_runs(r.id));
    } catch (err) {
      toast(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <main className="page">
      <header className="page-head">
        <h1>routines</h1>
        <span className="sub">{(routines ?? []).length} routine{(routines ?? []).length === 1 ? "" : "s"}</span>
        <div className="head-actions">
          <button className="primary" onClick={() => setDraft({ ...EMPTY })}>
            <Plus size={14} /> new routine
          </button>
        </div>
      </header>
      {project_id == null && (
        <p className="empty">pick a project scope in the dock — routines are project-scoped for agents</p>
      )}
      <section aria-label="routine list" className="routine-list">
        {routines === null && <p className="empty">loading…</p>}
        {routines !== null && routines.length === 0 && (
          <p className="empty">no routines — a routine is recurring automation (cron), not a task</p>
        )}
        {(routines ?? []).map((r) => (
          <article key={r.id} className={`routine-row ${r.enabled ? "" : "disabled"}`}>
            <div className="routine-main">
              <span className="routine-name">{r.name}</span>
              <span className="routine-cron" title={r.cron}>
                <CalendarClock size={12} /> {cron_label(r.cron)}
              </span>
              {r.agent && (
                <span className="routine-agent">
                  <Bot size={12} /> {r.agent}
                </span>
              )}
              {!r.enabled && <span className="routine-off">paused</span>}
            </div>
            <div className="routine-actions">
              <button onClick={() => open_runs(r)} aria-label={`runs of ${r.name}`}>runs</button>
              <button disabled={runningId === r.id} onClick={() => run_now(r)} aria-label={`run ${r.name} now`}>
                <Play size={12} /> {runningId === r.id ? "running…" : "run now"}
              </button>
              <button
                onClick={() =>
                  setDraft({
                    id: r.id,
                    name: r.name,
                    cron: r.cron,
                    agent: r.agent,
                    instruction: r.instruction,
                    enabled: r.enabled,
                  })
                }
                aria-label={`edit ${r.name}`}
              >
                <Pencil size={12} />
              </button>
              <button className="danger" onClick={() => remove(r.id)} aria-label={`delete ${r.name}`}>
                <Trash2 size={12} />
              </button>
            </div>
          </article>
        ))}
      </section>

      <Modal open={draft != null} title={draft?.id == null ? "new routine" : "edit routine"} on_close={() => setDraft(null)}>
        <form className="modal-form" onSubmit={save}>
          <label htmlFor="routine-name">name</label>
          <input
            id="routine-name"
            autoFocus
            value={draft?.name ?? ""}
            onChange={(e) => setDraft((d) => (d ? { ...d, name: e.target.value } : d))}
            placeholder="e.g. summarize rust news"
            required
          />
          <label>schedule</label>
          <RoutineEditor
            cron={draft?.cron ?? null}
            on_save={(expr) => setDraft((d) => (d ? { ...d, cron: expr || d.cron } : d))}
            next_run={null}
          />
          <label htmlFor="routine-agent">agent</label>
          <select
            id="routine-agent"
            value={draft?.agent ?? ""}
            onChange={(e) => setDraft((d) => (d ? { ...d, agent: e.target.value } : d))}
          >
            <option value="">(default engine)</option>
            {agents.map((a) => (
              <option key={a.name} value={a.name}>{a.name}</option>
            ))}
          </select>
          <label htmlFor="routine-instruction">instruction</label>
          <textarea
            id="routine-instruction"
            rows={3}
            value={draft?.instruction ?? ""}
            onChange={(e) => setDraft((d) => (d ? { ...d, instruction: e.target.value } : d))}
            placeholder="what this routine does each run…"
          />
          <label className="routine-enabled">
            <input
              type="checkbox"
              checked={draft?.enabled ?? true}
              onChange={(e) => setDraft((d) => (d ? { ...d, enabled: e.target.checked } : d))}
            />{" "}
            enabled
          </label>
          <button type="submit">save routine</button>
        </form>
      </Modal>

      <Modal open={runsFor != null} title={`runs — ${runsFor?.name ?? ""}`} on_close={() => setRunsFor(null)}>
        {runs === null && <p className="empty">loading…</p>}
        {runs !== null && runs.length === 0 && <p className="empty">no runs yet</p>}
        {runs !== null && runs.length > 0 && (
          <div className="routine-runs">
            {runs.map((run) => (
              <div key={run.id} className="routine-run-row">
                <span className="routine-run-time">{run.started_at.replace("T", " ")}</span>
                <span className="routine-run-trigger">{run.trigger}</span>
                <span className={run.ok ? "run-ok-text" : "run-failed-text"}>{run.ok ? "ok" : "failed"}</span>
                <span className="routine-run-summary" title={run.summary}>{run.summary}</span>
              </div>
            ))}
          </div>
        )}
      </Modal>
    </main>
  );
}
