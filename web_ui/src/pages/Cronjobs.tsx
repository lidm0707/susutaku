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

  async function save_schedule(card_id: number, cron: string | null) {
    try {
      await set_card_schedule(card_id, cron);
      setAdding(false);
      setCardPick("");
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

  return (
    <main className="page">
      <header className="page-head">
        <h1>
          <Clock size={16} /> cronjobs
        </h1>
        <button type="button" onClick={() => setAdding(true)}>
          schedule a card
        </button>
      </header>

      <section aria-label="scheduled cards">
        {jobs.length === 0 ? (
          <p className="empty">no scheduled cards — attach a pipeline to a card, then schedule it</p>
        ) : (
          <ul className="cron-list">
            {jobs.map((job) => (
              <li key={job.card_id}>
                <div>
                  <strong>{job.title}</strong>
                  <span className="cron-meta">
                    <code>{job.cron}</code>
                    {job.pipeline_name ? ` · ${job.pipeline_name}` : ""}
                  </span>
                </div>
                <div className="cron-actions">
                  <span title="next run">
                    <Timer size={14} /> {next_run_label(job.next_run)}
                  </span>
                  <button type="button" title="run now" onClick={() => run_now(job.card_id)}>
                    <Play size={14} />
                  </button>
                  <button
                    type="button"
                    title="unschedule"
                    onClick={() => save_schedule(job.card_id, null)}
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>

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
          <select value={cardPick} onChange={(e) => setCardPick(e.target.value)}>
            <option value="">pick a card with a pipeline…</option>
            {schedulable.map((c) => (
              <option key={c.id} value={c.id}>
                {c.title}
              </option>
            ))}
          </select>
          <select value={cronPick} onChange={(e) => setCronPick(e.target.value)}>
            {CRON_PRESETS.map((p) => (
              <option key={p.expr} value={p.expr}>
                {p.label} ({p.expr})
              </option>
            ))}
            <option value={CUSTOM}>custom…</option>
          </select>
          {cronPick === CUSTOM && (
            <input
              placeholder="cron expression, e.g. 30 4 * * 1-5"
              value={customCron}
              onChange={(e) => setCustomCron(e.target.value)}
            />
          )}
          <button type="submit">schedule</button>
        </form>
      </Modal>
    </main>
  );
}
