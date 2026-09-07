import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ArrowLeft, Box, Trash2 } from "lucide-react";
import { fetch_sandboxes, purge_sandbox, sweep_sandboxes, type SandboxDir } from "../lib.js";

export default function Sandbox() {
  const [dirs, setDirs] = useState<SandboxDir[]>([]);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    refresh();
  }, []);

  async function refresh() {
    try {
      setDirs(await fetch_sandboxes());
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function remove(pid: number) {
    setError("");
    setStatus("");
    try {
      const { removed } = await purge_sandbox(pid);
      setStatus(removed ? `removed sandbox ${pid}` : `sandbox ${pid} already gone`);
      await refresh();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function sweep() {
    setError("");
    setStatus("");
    try {
      const { removed } = await sweep_sandboxes();
      setStatus(`swept ${removed} stale sandbox${removed === 1 ? "" : "es"}`);
      await refresh();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  const stale = dirs.filter((d) => !d.alive).length;
  return (
    <main className="chat">
      <header>
        <h1><Link to="/sandbox">sandbox</Link></h1>
        <span className="sub">{dirs.length} dir{dirs.length === 1 ? "" : "s"} · {stale} stale</span>
        <nav className="nav">
          <button className="codex-login" onClick={sweep} title="remove dirs of dead backends">
            <Trash2 size={14} /> sweep stale
          </button>
          <Link to="/chat" title="back to chat"><ArrowLeft size={16} /></Link>
        </nav>
      </header>
      {status && <p className="saved-mark" style={{ textAlign: "center" }}>{status}</p>}
      {error && <p className="error">{error}</p>}
      <section className="log">
        {dirs.length === 0 && <p className="empty">No sandbox dirs found.</p>}
        {dirs.map((d) => (
          <button
            key={d.pid}
            className="model-row"
            onClick={() => !d.alive && remove(d.pid)}
            disabled={d.alive}
            title={d.alive ? "owned by a running backend" : "stale — click to remove"}
          >
            <Box size={14} />
            <span className="model-name">pid {d.pid}</span>
            <span className="model-meta" style={{ justifyContent: "flex-start" }}>
              {d.path}
            </span>
            <span className={`codex-login ${d.alive ? "ok" : "warn"}`}>
              {d.alive ? "alive" : "stale"}
            </span>
          </button>
        ))}
      </section>
    </main>
  );
}
