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
const DEFAULT_PRESET = CRON_PRESETS[5].expr;

enum RoutineUnit {
  Minutes = "minutes",
  Hours = "hours",
  Days = "days",
}

const ROUTINE_UNITS = [
  { unit: RoutineUnit.Minutes, label: "minutes" },
  { unit: RoutineUnit.Hours, label: "hours" },
  { unit: RoutineUnit.Days, label: "days" },
] as const;

const WEEKDAYS = [
  { value: 0, label: "sunday" },
  { value: 1, label: "monday" },
  { value: 2, label: "tuesday" },
  { value: 3, label: "wednesday" },
  { value: 4, label: "thursday" },
  { value: 5, label: "friday" },
  { value: 6, label: "saturday" },
] as const;

const CRON_SAMPLES = ["*/15 * * * *", "0 9 * * 1-5", "30 4 1,15 * *", "0 3 * * 0"] as const;

const CRON_FIELD_COUNT = 5;
const MINUTE_MAX = 59;
const HOUR_MAX = 23;
const ROUTINE_DEFAULT = 15;
const DAY_STEP_MAX = 31;
const DEFAULT_TIME = "09:00";

const CRON_FIELDS = [
  { min: 0, max: MINUTE_MAX },
  { min: 0, max: HOUR_MAX },
  { min: 1, max: 31 },
  { min: 1, max: 12 },
  { min: 0, max: 7 },
] as const;

type FieldSpec = (typeof CRON_FIELDS)[number];

function cron_field_ok(part: string, spec: FieldSpec): boolean {
  const [range, step] = part.split("/");
  if (step !== undefined && (!/^\d+$/.test(step) || Number(step) === 0)) return false;
  if (range === "*") return true;
  if (range.includes(",")) return range.split(",").every((p) => cron_field_ok(p, spec));
  if (range.includes("-")) {
    const [a, b] = range.split("-");
    const lo = Number(a);
    const hi = Number(b);
    return /^\d+$/.test(a) && /^\d+$/.test(b) && lo <= hi && lo >= spec.min && hi <= spec.max;
  }
  if (!/^\d+$/.test(range)) return false;
  const n = Number(range);
  return n >= spec.min && n <= spec.max;
}

function cron_valid(expr: string): boolean {
  const parts = expr.trim().split(/\s+/);
  return (
    parts.length === CRON_FIELD_COUNT &&
    parts.every((p, i) => cron_field_ok(p, CRON_FIELDS[i]))
  );
}

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

function time_parts(time: string): [number, number] {
  const [h, m] = time.split(":").map(Number);
  const hour = Number.isInteger(h) ? Math.min(Math.max(h, 0), HOUR_MAX) : 0;
  const min = Number.isInteger(m) ? Math.min(Math.max(m, 0), MINUTE_MAX) : 0;
  return [hour, min];
}

function build_cron(unit: RoutineUnit, n: number, time: string): string {
  const [h, m] = time_parts(time);
  const step = Math.max(1, n);
  switch (unit) {
    case RoutineUnit.Minutes:
      return m === 0 ? `*/${Math.min(step, MINUTE_MAX)} * * * *` : `${m}-59/${Math.min(step, MINUTE_MAX)} * * * *`;
    case RoutineUnit.Hours:
      return `${m} ${h === 0 ? `*/${Math.min(step, HOUR_MAX)}` : `${h}-23/${Math.min(step, HOUR_MAX)}`} * * *`;
    case RoutineUnit.Days:
      return `${m} ${h} */${Math.min(step, DAY_STEP_MAX)} * *`;
  }
}

function step_of(part: string): [string, string | null] {
  const [range, step] = part.split("/");
  return [range, step ?? null];
}

function describe_cron(expr: string): string {
  if (!cron_valid(expr)) return "invalid cron expression";
  const [min, hour, dom, mon, dow] = expr.trim().split(/\s+/);
  const at = `at ${pad2(Number(hour.split("-")[0]))}:${pad2(Number(min.split("-")[0].split("/")[0]))}`;
  const weekday = WEEKDAYS[Number(dow.split("-")[0].split("/")[0]) % 7]?.label ?? `day ${dow}`;
  const [minRange, minStep] = step_of(min);
  const [hourRange, hourStep] = step_of(hour);
  const [, domStep] = step_of(dom);
  if (hour === "*" && dom === "*" && mon === "*") {
    if (dow === "*") {
      if (min === "*") return "every minute";
      if (minStep !== null) {
        const from = minRange === "*" ? "" : ` starting at :${pad2(Number(minRange.split("-")[0]))}`;
        return `every ${minStep} minutes${from}`;
      }
      return `hourly at minute ${min}`;
    }
    return `every hour, on ${weekday}`;
  }
  if (min === "*" || hour === "*") return "per cron schedule";
  if (dom === "*" && mon === "*") {
    if (dow === "*") {
      if (hourStep !== null) {
        const from = hourRange === "*" ? "" : ` from ${pad2(Number(hourRange.split("-")[0]))}:00`;
        return `every ${hourStep} hours${from}, at minute ${min}`;
      }
      return `daily ${at}`;
    }
    return `weekly on ${weekday} ${at}`;
  }
  if (mon === "*" && domStep !== null) return `every ${domStep} days ${at}`;
  if (mon === "*") return `monthly on day ${dom} ${at}`;
  return `yearly on ${mon}/${dom} ${at}`;
}

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
  const [cronPick, setCronPick] = useState<string>(DEFAULT_PRESET);
  const [routineUnit, setRoutineUnit] = useState<RoutineUnit>(RoutineUnit.Minutes);
  const [routineN, setRoutineN] = useState(ROUTINE_DEFAULT);
  const [startTime, setStartTime] = useState(DEFAULT_TIME);
  const [showRaw, setShowRaw] = useState(false);
  const [rawCron, setRawCron] = useState("");

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

  const unscheduled = cards.filter((c) => !c.cron);
  const active = jobs.find((j) => j.card_id === selected) ?? null;

  const builtCron = build_cron(routineUnit, routineN, startTime);
  const customCron = showRaw ? rawCron.trim() : builtCron;
  const customCronValid = cron_valid(customCron);
  const customPreview = describe_cron(customCron);

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
            const cron = cronPick === CUSTOM ? customCron : cronPick;
            if (cron && (cronPick !== CUSTOM || customCronValid)) save_schedule(Number(cardPick), cron);
          }}
        >
          <label className="modal-label">
            card
            <select value={cardPick} onChange={(e) => setCardPick(e.target.value)}>
              <option value="">pick a card with a pipeline…</option>
              {unscheduled.length === 0 && <option disabled>no cards yet</option>}
              {unscheduled.map((c) => (
                <option key={c.id} value={c.pipeline_id != null ? c.id : ""} disabled={c.pipeline_id == null}>
                  {c.pipeline_id == null ? `${c.title} — no pipeline attached` : c.title}
                </option>
              ))}
            </select>
          </label>
          <label className="modal-label">
            schedule
            <select
              value={cronPick}
              onChange={(e) => {
                setCronPick(e.target.value);
                if (e.target.value === CUSTOM) setShowRaw(true);
              }}
            >
              {CRON_PRESETS.map((p) => (
                <option key={p.expr} value={p.expr}>
                  {p.label} ({p.expr})
                </option>
              ))}
              <option value={CUSTOM}>advanced · manual cron expression…</option>
            </select>
          </label>
          {cronPick === CUSTOM && (
            <div className="cron-builder">
              <label className="modal-label">
                start time
                <input type="time" value={startTime} onChange={(e) => setStartTime(e.target.value)} />
              </label>
              <label className="modal-label">
                routine (every)
                <span className="cron-routine">
                  <input
                    type="number"
                    min={1}
                    max={routineUnit === RoutineUnit.Minutes ? MINUTE_MAX : routineUnit === RoutineUnit.Hours ? HOUR_MAX : DAY_STEP_MAX}
                    value={routineN}
                    onChange={(e) => setRoutineN(Number(e.target.value))}
                  />
                  <select
                    value={routineUnit}
                    onChange={(e) => setRoutineUnit(e.target.value as RoutineUnit)}
                  >
                    {ROUTINE_UNITS.map((u) => (
                      <option key={u.unit} value={u.unit}>{u.label}</option>
                    ))}
                  </select>
                </span>
              </label>
              <button type="button" className="cron-raw-toggle" onClick={() => setShowRaw((v) => !v)}>
                {showRaw ? "hide raw cron" : "raw cron…"}
              </button>
              {showRaw && (
                <>
                  <label className="modal-label">
                    cron expression
                    <input
                      placeholder="30 4 * * 1-5"
                      value={rawCron}
                      onChange={(e) => setRawCron(e.target.value)}
                    />
                  </label>
                  <div className="cron-samples">
                    {CRON_SAMPLES.map((s) => (
                      <button key={s} type="button" onClick={() => setRawCron(s)}>{s}</button>
                    ))}
                  </div>
                </>
              )}
              <p className={customCronValid ? "cron-preview" : "cron-preview invalid"}>
                <code>{customCron || "—"}</code> · {customPreview}
              </p>
            </div>
          )}
          <button type="submit" disabled={cronPick === CUSTOM && !customCronValid}>schedule</button>
        </form>
      </Modal>
    </main>
  );
}
