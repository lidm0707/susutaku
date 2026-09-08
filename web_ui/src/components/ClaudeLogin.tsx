import { useEffect, useRef, useState } from "react";
import { LogIn, ShieldCheck } from "lucide-react";
import { claude_callback, claude_start, claude_status, CodexStatus } from "../lib.js";
import { Modal } from "../ui/Overlay.tsx";

export default function ClaudeLogin() {
  const [status, setStatus] = useState<CodexStatus["status"]>("missing");
  const [cliOk, setCliOk] = useState(true);
  const [busy, setBusy] = useState(false);
  const [pasting, setPasting] = useState(false);
  const [error, setError] = useState("");
  const code = useRef("");
  const authorizing = useRef(false);

  useEffect(() => {
    refresh();
  }, []);

  async function refresh(): Promise<void> {
    try {
      apply(await claude_status());
    } catch {
      /* backend unreachable — keep last known state */
    }
  }

  function apply(reply: CodexStatus): void {
    setStatus(reply.status);
    setCliOk(reply.cli_available !== false);
    setError(reply.error || (reply.cli_available === false ? "claude CLI not found on backend — chat via claude will fail" : ""));
    if (reply.status !== "awaiting_login") setBusy(false);
  }

  async function login(): Promise<void> {
    setError("");
    setBusy(true);
    try {
      const { authorize_url } = await claude_start();
      window.open(authorize_url, "claude-login", "width=520,height=720");
      authorizing.current = true;
      setPasting(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function submit_code(): Promise<void> {
    const value = code.current.trim();
    if (!value) return;
    setError("");
    setBusy(true);
    try {
      await claude_callback(value);
      authorizing.current = false;
      setPasting(false);
      code.current = "";
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  const logged_in = status === "logged_in";
  const warn = !cliOk;
  return (
    <>
      <button
        className={`codex-login ${logged_in ? "ok" : ""} ${warn ? "warn" : ""}`}
        onClick={logged_in && !warn ? undefined : login}
        disabled={busy}
        title={error || (logged_in ? "claude: signed in" : "sign in with claude.ai")}
      >
        {logged_in && !warn ? <ShieldCheck size={14} /> : <LogIn size={14} />}
        {logged_in && !warn ? "claude" : warn ? "claude (no cli)" : "sign in with claude"}
      </button>
      <Modal open={pasting} title="claude login" on_close={() => { setPasting(false); authorizing.current = false; }}>
        <form
          className="modal-form"
          onSubmit={(e) => {
            e.preventDefault();
            void submit_code();
          }}
        >
          <label className="modal-label">
            paste the code shown after approving claude.ai access
            <input
              autoFocus
              placeholder="code#state"
              onChange={(e) => (code.current = e.target.value)}
            />
          </label>
          {error && <p className="error">{error}</p>}
          <button type="submit" disabled={busy || !authorizing.current}>
            {authorizing.current ? "exchange code" : "start login first"}
          </button>
        </form>
      </Modal>
    </>
  );
}
