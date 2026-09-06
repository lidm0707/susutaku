import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ArrowLeft, Check, HardDrive, Snowflake, Zap } from "lucide-react";
import { fetch_models, pretty_name, select_model, size_label } from "../lib.js";

export function EngineIcon({ engine }) {
  return engine === "gguf" ? <Snowflake size={14} /> : <Zap size={14} />;
}

export default function Models() {
  const [models, setModels] = useState([]);
  const [error, setError] = useState("");

  useEffect(() => {
    fetch_models().then(setModels).catch((err) => setError(String(err.message || err)));
  }, []);

  async function pick(name) {
    setError("");
    try {
      await select_model(name);
      setModels((list) => list.map((m) => ({ ...m, selected: m.name === name })));
    } catch (err) {
      setError(String(err.message || err));
    }
  }

  return (
    <main className="chat">
      <header>
        <h1><Link to="/models">models</Link></h1>
        <span className="sub">{models.length} available</span>
        <nav className="nav">
          <Link to="/" title="back to chat"><ArrowLeft size={16} /></Link>
        </nav>
      </header>
      {error && <p className="error">{error}</p>}
      <section className="log">
        {models.length === 0 && <p className="empty">No models found under models/.</p>}
        {models.map((m) => (
          <button
            key={m.name}
            className="model-row"
            onClick={() => pick(m.name)}
            disabled={!m.loadable}
            title={m.loadable ? "switch to this model" : "not loadable (engine/quant filter)"}
          >
            <EngineIcon engine={m.engine} />
            <span className="model-name">{pretty_name(m.name)}</span>
            <span className="model-meta">
              {m.engine.toUpperCase()} · <HardDrive size={12} /> {size_label(m.bytes)}
            </span>
            {m.selected && <Check size={16} className="selected-mark" />}
          </button>
        ))}
      </section>
    </main>
  );
}
