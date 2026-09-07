import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ArrowLeft, Bot, KeyRound, Workflow } from "lucide-react";
import { fetch_zai_settings, save_zai_settings } from "../lib.js";

export default function Settings() {
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [keySet, setKeySet] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    fetch_zai_settings()
      .then((s) => {
        setKeySet(s.api_key_set);
        setModel(s.model || "");
      })
      .catch((err) => setError(String(err.message || err)));
  }, []);

  async function save(e) {
    e.preventDefault();
    setStatus("");
    setError("");
    try {
      const s = await save_zai_settings(apiKey, model);
      setKeySet(s.api_key_set);
      setModel(s.model || "");
      setApiKey("");
      setStatus("saved");
    } catch (err) {
      setError(String(err.message || err));
    }
  }

  return (
    <main className="chat">
      <header>
        <h1><Link to="/settings">settings</Link></h1>
        <span className="sub">z.ai · {keySet ? "key saved" : "no key"}</span>
        <nav className="nav">
          <Link to="/pipelines" title="pipelines"><Workflow size={16} /></Link>
          <Link to="/agents" title="saved agents"><Bot size={16} /></Link>
          <Link to="/" title="back to chat"><ArrowLeft size={16} /></Link>
        </nav>
      </header>
      <section className="log">
        <form className="settings-form" onSubmit={save}>
          <label>
            <KeyRound size={14} /> api key {keySet && <em className="saved-mark">(saved)</em>}
          </label>
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder={keySet ? "•••••••• (type to replace)" : "paste your z.ai api key"}
            autoComplete="off"
          />
          <label>model</label>
          <input
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="glm-4.6"
          />
          <button type="submit">save</button>
          {status && <span className="saved-mark">{status}</span>}
        </form>
        {error && <p className="error">{error}</p>}
      </section>
    </main>
  );
}
