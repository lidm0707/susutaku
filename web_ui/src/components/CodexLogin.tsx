import { useEffect, useRef, useState } from "react";
import { LogIn, ShieldCheck } from "lucide-react";
import { codex_start, codex_status, CodexStatus } from "../lib.js";

const POLL_MS = 2000;
const MAX_POLLS = 150;

export default function CodexLogin() {
  const [status, setStatus] = useState<CodexStatus["status"]>("missing");
  const [cliOk, setCliOk] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);
  const polls = useRef(0);

  useEffect(() => {
    refresh();
    return () => clearInterval(timer.current!);
  }, []);

  async function refresh(): Promise<void> {
    try {
      apply(await codex_status());
    } catch {
      /* backend unreachable — keep last known state */
    }
  }

  function apply(reply: CodexStatus): void {
    setStatus(reply.status);
    setCliOk(reply.cli_available !== false);
    setError(reply.error || (reply.cli_available === false ? "codex CLI not found on backend — chat via codex will fail" : ""));
    if (reply.status !== "awaiting_login" || ++polls.current > MAX_POLLS) {
      clearInterval(timer.current!);
      timer.current = null;
      setBusy(false);
    }
  }

  async function login(): Promise<void> {
    setError("");
    setBusy(true);
    try {
      const { authorize_url } = await codex_start();
      window.open(authorize_url, "codex-login", "width=520,height=720");
      clearInterval(timer.current!);
      polls.current = 0;
      timer.current = setInterval(async () => {
        try {
          apply(await codex_status());
        } catch {
          /* transient poll failure */
        }
      }, POLL_MS);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setBusy(false);
    }
  }

  const logged_in = status === "logged_in";
  const warn = !cliOk;
  return (
    <button
      className={`codex-login ${logged_in ? "ok" : ""} ${warn ? "warn" : ""}`}
      onClick={logged_in && !warn ? undefined : login}
      disabled={busy}
      title={error || (logged_in ? "codex: signed in" : "sign in with codex/chatgpt")}
    >
      {logged_in && !warn ? <ShieldCheck size={14} /> : <LogIn size={14} />}
      {logged_in && !warn ? "codex" : warn ? "codex (no cli)" : busy ? "waiting…" : "sign in with codex"}
    </button>
  );
}
