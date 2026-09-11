import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { Clock, RefreshCw, X } from "lucide-react";
import { fetch_quota_board, type PlatformQuota } from "../lib.js";

const TZ_LOCAL = "local";
const TZ_KEY = "susutaku:tz";
const QUOTA_MINT_MAX = 50;
const QUOTA_LEMON_MAX = 80;
const DAY_MS = 86_400_000;

type BoardState = { rows: PlatformQuota[] } | { error: true } | null;

function stored_tz(): string {
  return localStorage.getItem(TZ_KEY) || TZ_LOCAL;
}

function quota_bar_class(pct: number): string {
  return pct < QUOTA_MINT_MAX ? "mint" : pct < QUOTA_LEMON_MAX ? "lemon" : "pink";
}

function fmt(ms: number, tz: string, with_date: boolean): string {
  const fmt = new Intl.DateTimeFormat("en-GB", {
    ...(with_date ? { day: "2-digit", month: "short", year: "numeric" } : {}),
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
    ...(tz !== TZ_LOCAL ? { timeZone: tz } : {}),
  });
  return fmt.format(new Date(ms));
}

/// Resets within a day → time only; further out → full date + time.
function reset_label(ms: number, tz: string): string {
  return fmt(ms, tz, ms - Date.now() > DAY_MS);
}

export default function QuotaBoard({ open, on_close }: { open: boolean; on_close: () => void }) {
  const [state, setState] = useState<BoardState>(null);
  const [busy, setBusy] = useState(false);

  async function load() {
    setBusy(true);
    try {
      setState({ rows: await fetch_quota_board() });
    } catch {
      setState({ error: true });
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    if (!open) return;
    load();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const on_key = (e: KeyboardEvent) => {
      if (e.key === "Escape") on_close();
    };
    document.addEventListener("keydown", on_key);
    return () => document.removeEventListener("keydown", on_key);
  }, [open, on_close]);

  if (!open) return null;

  const tz = stored_tz();

  return createPortal(
    <div className="quota-overlay" role="dialog" aria-modal="false" aria-label="quota board">
      <p className="quota-board-head">
        <span className="quota-board-title">quota board</span>
        <button type="button" className="icon-btn" title="refresh quota board" disabled={busy} onClick={load}>
          <RefreshCw size={14} />
        </button>
        <button type="button" className="icon-btn" title="close" onClick={on_close} aria-label="close quota board">
          <X size={14} />
        </button>
      </p>
      {state === null && <p className="model-empty">loading…</p>}
      {state !== null && "error" in state && <p className="model-empty">quota board unavailable</p>}
      {state !== null && !("error" in state) && (
        <ul className="quota-board-list">
          {state.rows.map((row) => (
            <QuotaRow key={row.platform} row={row} tz={tz} />
          ))}
        </ul>
      )}
    </div>,
    document.body,
  );
}

function QuotaRow({ row, tz }: { row: PlatformQuota; tz: string }) {
  return (
    <li className={`quota-row${row.available ? "" : " muted"}`}>
      <span className="quota-row-name">{row.platform}</span>
      {row.available ? (
        <>
          {row.tokens_used_pct !== null && (
            <span
              className="zai-quota-bar quota-row-bar"
              role="progressbar"
              aria-valuenow={row.tokens_used_pct}
              aria-valuemin={0}
              aria-valuemax={100}
            >
              <span
                className={`zai-quota-fill ${quota_bar_class(row.tokens_used_pct)}`}
                style={{ width: `${Math.min(100, Math.max(0, row.tokens_used_pct))}%` }}
              />
            </span>
          )}
          <span className="quota-row-info">
            {row.tokens_used_pct !== null && <>{row.tokens_used_pct}%</>}
            {row.window_used_pct !== null && <span> · win {row.window_used_pct}%</span>}
            {row.window_reset_ms !== null && (
              <span>
                {" "}
                · <Clock size={12} /> resets {reset_label(row.window_reset_ms, tz)}
              </span>
            )}
            {row.tokens_used_pct === null && row.window_used_pct === null && row.window_reset_ms === null && <>—</>}
          </span>
        </>
      ) : (
        <span className="quota-row-info">unavailable{row.reason ? ` — ${row.reason}` : ""}</span>
      )}
    </li>
  );
}

