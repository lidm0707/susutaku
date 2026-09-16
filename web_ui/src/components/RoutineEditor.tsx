import { useEffect, useState } from "react";
import { Clock, Play, Timer, Trash2 } from "lucide-react";

export const CRON_PRESETS = [
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

export function cron_valid(expr: string): boolean {
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

export function describe_cron(expr: string): string {
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

export function cron_label(expr: string | null | undefined): string {
  if (!expr) return "";
  return CRON_PRESETS.find((p) => p.expr === expr)?.label || expr;
}

export function next_run_label(secs: number): string {
  if (!secs) return "pending";
  const mins = Math.round((secs - Date.now() / 1000) / 60);
  if (mins <= 0) return "due now";
  if (mins < 60) return `in ${mins}m`;
  return `in ${Math.round(mins / 60)}h`;
}

function preset_of(expr: string): string {
  return CRON_PRESETS.some((p) => p.expr === expr) ? expr : CUSTOM;
}

/// Routine schedule editor: preset select + custom builder (every N
/// minutes/hours/days from a start time, or a raw cron expression).
/// `embedded` strips the chrome (label, clear/save buttons) and reports
/// every valid change live via `on_change` — for use inside another form.
export function RoutineEditor({
  cron,
  on_save,
  next_run,
  embedded,
  on_change,
}: {
  cron: string | null;
  on_save: (cron: string | null) => void;
  next_run: number | null;
  embedded?: boolean;
  on_change?: (cron: string) => void;
}) {
  const [cronPick, setCronPick] = useState(() => (cron ? preset_of(cron) : ""));
  const [routineUnit, setRoutineUnit] = useState<RoutineUnit>(RoutineUnit.Minutes);
  const [routineN, setRoutineN] = useState(ROUTINE_DEFAULT);
  const [startTime, setStartTime] = useState(DEFAULT_TIME);
  const [showRaw, setShowRaw] = useState(() => cron != null && !CRON_PRESETS.some((p) => p.expr === cron));
  const [rawCron, setRawCron] = useState(cron ?? "");

  useEffect(() => {
    setCronPick(cron ? preset_of(cron) : "");
    setRawCron(cron ?? "");
    setShowRaw(cron != null && !CRON_PRESETS.some((p) => p.expr === cron));
  }, [cron]);

  const builtCron = build_cron(routineUnit, routineN, startTime);
  const customCron = showRaw ? rawCron.trim() : builtCron;
  const customCronValid = cron_valid(customCron);
  const customPreview = describe_cron(customCron);

  // Embedded mode: stream every valid custom expression up as it is edited.
  useEffect(() => {
    if (embedded && cronPick === CUSTOM && customCronValid && on_change) {
      on_change(customCron);
    }
  }, [embedded, cronPick, customCron, customCronValid, on_change]);

  function pick(value: string) {
    if (value === "") {
      if (cron) on_save(null);
      return;
    }
    if (value === CUSTOM) {
      setCronPick(CUSTOM);
      setShowRaw(true);
      return;
    }
    setCronPick(value);
    setShowRaw(false);
    on_save(value);
  }

  function save_custom(e: React.FormEvent) {
    e.preventDefault();
    if (customCronValid) on_save(customCron);
  }

  const active_desc = cron ? describe_cron(cron) : null;

  return (
    <div className={embedded ? "routine-editor embedded" : "routine-editor"}>
      {!embedded && (
        <label className="modal-label" htmlFor="task-detail-schedule">
          <Clock size={13} /> routine
        </label>
      )}
      <select
        id={embedded ? "routine-schedule" : "task-detail-schedule"}
        className="task-select"
        value={cronPick}
        onChange={(e) => pick(e.target.value)}
        title="routine agent runs (cron)"
      >
        <option value="">no routine…</option>
        {CRON_PRESETS.map((p) => (
          <option key={p.expr} value={p.expr}>{p.label}</option>
        ))}
        <option value={CUSTOM}>custom routine…</option>
      </select>
      {cron && (
        <p className="task-auto-hint">
          {active_desc}
          {next_run != null && (
            <>
              {" · "}
              <Timer size={11} /> next run {next_run_label(next_run)}
            </>
          )}
        </p>
      )}
      {cron && !embedded && (
        <button type="button" className="task-run-inline" onClick={() => on_save(null)}>
          <Trash2 size={13} /> clear routine
        </button>
      )}
      {cronPick === CUSTOM && (
        <form className="cron-builder" onSubmit={save_custom}>
          <label className="modal-label">
            start time
            <input type="time" value={startTime} onChange={(e) => setStartTime(e.target.value)} />
          </label>
          <label className="modal-label">
            every
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
          {!embedded && (
            <button type="submit" disabled={!customCronValid}>
              <Play size={13} /> save routine
            </button>
          )}
        </form>
      )}
    </div>
  );
}
