import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Clock, Play, Timer, Trash2 } from "lucide-react";
import {
  clear_token,
  fetch_cards,
  fetch_cronjobs,
  run_card,
  set_card_schedule,
  type Card,
  type CronJob,
} from "../lib.js";
import { Modal } from "../ui/Overlay.js";
import { Button } from "../ui/controls.js";

const CRON_PRESETS = [
  { expr: "* * * * *", label: "every minute" },
  { expr: "*/5 * * * *", label: "every 5 minutes" },
  { expr: "*/15 * * * *", label: "every 15 minutes" },
  { expr: "0 * * * *", label: "hourly" },
  { expr: "0 */6 * * *", label: "every 6 hours" },
  { expr: "0 3 * * *", label: "daily 03:00" },
  { expr: "0 3 * * 1", label: "mondays 03:00" },
  { expr: "0 9 1 * *", label: "monthly, 1st 09:00" },
] as const;

const CUSTOM = "__custom__";

function next_run_label(secs: number): string {
  if (!secs) return "pending";
  const mins = Math.round((secs - Date.now() / 1000) / 60);
  if (mins <= 0) return "due now";
  if (mins < 60) return `in ${mins}m`;
  return `in ${Math.round(mins / 60)}h`;
}

export default function Cronjobs() {
  const nav = useNavigate();
  const [jobs, setJobs] = useState<CronJob[]>([]);
  const [cards, setCards] = useState<Card[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [adding, setAdding] = useState(false);
  const [cardPick, setCardPick] = useState("");
  const [cronPick, setCronPick] = useState<string>(CRON_PRESETS[1].expr);
  const [customCron, setCustomCron] = useState("");

  const handle = useCallback(
    (err: unknown) => {
      if (err instanceof Object && "status" in err && (err as { status?: number }).status === 401) {
        clear_token();
        nav("/");
        return;
      }
    },
    [nav]
  );

  const refresh = useCallback(async () => {
    try {
      const [jobs, cards] = await Promise.all([fetch_cronjobs(), fetch_cards(null)]);
      setJobs(jobs);
      setCards(cards);
    } catch (err) {
      handle(err);
    }
  }, [handle]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    if (selected != null && !jobs.some((j) => j.card_id === selected)) setSelected(null);
  }, [jobs, selected]);

  async function save_schedule(card_id: number, cron: string | null) {
    try {
      await set_card_schedule(card_id, cron);
      setAdding(false);
      setCardPick("");
      if (cron === null) setSelected(null);
      refresh();
    } catch (err) {
      handle(err);
    }
  }

  async function run_now(card_id: number) {
    try {
      await run_card(card_id);
      refresh();
    } catch (err) {
      handle(err);
    }
  }

  const schedulable = cards.filter((c) => c.pipeline_id != null && !c.cron);
  const active = jobs.find((j) => j.card_id === selected) ?? null;

  return (
    <main className="chat kanban-page agents-page">
      <header>
        <h1>cronjobs</h1>
        <span className="sub">{jobs.length} scheduled</span>
      </header>
      <div className="agents-layout">
        <aside className="agents-side">
          <Button variant="ghost" className="agents-new" onClick={() => setAdding(true)}>
            <Clock size={14} /> schedule a card
          </Button>
          <div className="agents-list">
            {jobs.length === 0 && (
              <span className="agents-empty">no scheduled cards yet</span>
            )}
            {jobs.map((job) => (
              <button
                key={job.card_id}
                className={selected === job.card_id ? "agent-item active" : "agent-item"}
                onClick={() => setSelected(job.card_id)}
              >
                <Clock size={14} />
                <span className="agent-item-name">{job.title}</span>
                <span className="agent-item-model">{job.cron}</span>
              </button>
            ))}
          </div>
        </aside>
        {active ? (
          <section className="agent-editor" aria-label={`schedule for ${active.title}`}>
            <header className="agent-editor-row">
              <h2>{active.title}</h2>
            </header>
            <div className="agent-editor-row">
              <div>
                <span className="agent-item-name">schedule</span>
                <p className="cron-meta">
                  <code>{active.cron}</code>
                  {active.pipeline_name ? ` · ${active.pipeline_name}` : ""}
                </p>
              </div>
              <div>
                <span className="agent-item-name">next run</span>
                <p className="cron-meta">
                  <Timer size={14} /> {next_run_label(active.next_run)}
                </p>
              </div>
            </div>
            <footer className="agent-editor-foot">
              <Button variant="primary" type="button" onClick={() => run_now(active.card_id)}>
                <Play size={14} /> run now
              </Button>
              <Button
                variant="danger"
                type="button"
                onClick={() => save_schedule(active.card_id, null)}
              >
                <Trash2 size={14} /> unschedule
              </Button>
            </footer>
          </section>
        ) : (
          <div className="agent-editor agent-editor-empty">
            <Clock size={28} />
            <p>
              select a scheduled card on the left,
              <br />
              or schedule a new one.
            </p>
            <Button variant="ghost" onClick={() => setAdding(true)}>
              <Clock size={14} /> schedule a card
            </Button>
          </div>
        )}
      </div>

      <Modal open={adding} title="schedule a card" on_close={() => setAdding(false)}>
        <form
          className="modal-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (!cardPick) return;
            const cron = cronPick === CUSTOM ? customCron.trim() : cronPick;
            if (cron) save_schedule(Number(cardPick), cron);
          }}
        >
          <label className="modal-label">
            card
            <select value={cardPick} onChange={(e) => setCardPick(e.target.value)}>
              <option value="">pick a card with a pipeline…</option>
              {schedulable.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.title}
                </option>
              ))}
            </select>
          </label>
          <label className="modal-label">
            schedule
            <select value={cronPick} onChange={(e) => setCronPick(e.target.value)}>
              {CRON_PRESETS.map((p) => (
                <option key={p.expr} value={p.expr}>
                  {p.label} ({p.expr})
                </option>
              ))}
              <option value={CUSTOM}>custom…</option>
            </select>
          </label>
          {cronPick === CUSTOM && (
            <label className="modal-label">
              cron expression
              <input
                placeholder="30 4 * * 1-5"
                value={customCron}
                onChange={(e) => setCustomCron(e.target.value)}
              />
            </label>
          )}
          <button type="submit">schedule</button>
        </form>
      </Modal>
    </main>
  );
}
