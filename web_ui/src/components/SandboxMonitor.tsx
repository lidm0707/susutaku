import { useEffect, useRef, useState } from "react";
import { X } from "lucide-react";
import { fetch_sandbox_logs, type SandboxLogEntry } from "../lib.js";

const POLL_TICK_MS = 5000;
const TRANSCRIPT_MAX_ENTRIES = 200;

export type SandboxMonitorTarget = {
  machine: string;
  path: string;
  pid: number;
  alive: boolean;
};

type Props = {
  target: SandboxMonitorTarget | null;
  on_close: () => void;
};

export function SandboxMonitorOverlay({ target, on_close }: Props) {
  const [logs, set_logs] = useState<SandboxLogEntry[] | null>(null);
  const pre_ref = useRef<HTMLPreElement>(null);

  useEffect(() => {
    if (!target) return;
    set_logs(null);
    let alive = true;
    const tick = () => {
      fetch_sandbox_logs(target.path)
        .then((l) => {
          if (!alive) return;
          set_logs(l);
        })
        .catch(() => {});
    };
    tick();
    const t = setInterval(tick, POLL_TICK_MS);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, [target]);

  useEffect(() => {
    const pre = pre_ref.current;
    if (pre) pre.scrollTop = pre.scrollHeight;
  }, [logs]);

  if (!target) return null;

  const entries = (logs ?? []).slice(-TRANSCRIPT_MAX_ENTRIES);

  return (
    <aside className="sandbox-monitor" aria-label="sandbox monitor" role="status">
      <div className="sandbox-monitor-head">
        <span className={`dot ${target.alive ? "alive" : "stale"}`} aria-hidden="true" />
        <span className="sandbox-monitor-title">sandbox {target.machine} (pid {target.pid})</span>
        <button className="sandbox-monitor-close" onClick={on_close} title="close monitor" aria-label="close monitor">
          <X size={12} />
        </button>
      </div>
      <pre className="agent-logs-pre" ref={pre_ref}>
        {entries.length === 0 ? "no transcript yet…" : entries.map((e) => e.content).join("\n")}
      </pre>
    </aside>
  );
}
