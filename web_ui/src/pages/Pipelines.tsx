import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { ArrowLeft, Plus, Trash2 } from "lucide-react";
import {
  clear_token,
  create_pipeline,
  fetch_pipelines,
  remove_pipeline,
  update_pipeline,
  type Pipeline,
} from "../lib.js";

const STAGES = ["ingest", "parse", "transform", "model_infer", "render", "custom"] as const;

type Stage = (typeof STAGES)[number];

type NodeRow = { id: string; stage: string; params: string };
type LinkRow = { from: string; to: string };

export default function Pipelines() {
  const nav = useNavigate();
  const [pipelines, setPipelines] = useState<Pipeline[]>([]);
  const [selected, setSelected] = useState<number | "new" | null>(null);
  const [name, setName] = useState("");
  const [nodes, setNodes] = useState<NodeRow[]>([]);
  const [links, setLinks] = useState<LinkRow[]>([]);
  const [, setError] = useState("");
  const [status, setStatus] = useState("");

  useEffect(() => {
    load_pipelines();
  }, []);

  async function handle(err: unknown) {
    if (err instanceof Object && "status" in err && (err as { status?: number }).status === 401) {
      clear_token();
      nav("/");
      return;
    }
    setError(err instanceof Error ? err.message : String(err));
  }

  async function load_pipelines() {
    try {
      setPipelines(await fetch_pipelines());
    } catch (err) {
      handle(err);
    }
  }

  function pick(p: Pipeline) {
    setError("");
    setStatus("");
    setSelected(p.id);
    setName(p.name);
    setNodes(p.spec?.nodes?.map((n) => ({ id: n.id || "", stage: n.stage || "custom", params: JSON.stringify(n.params ?? {}, null, 2) })) || []);
    setLinks(p.spec?.links?.map((l) => ({ from: l.from || "", to: l.to || "" })) || []);
  }

  function start_new() {
    setError("");
    setStatus("");
    setSelected("new");
    setName("");
    setNodes([{ id: "", stage: "ingest", params: "{}" }]);
    setLinks([]);
  }

  function set_node(i: number, patch: Partial<NodeRow>) {
    setNodes(nodes.map((n, j) => (j === i ? { ...n, ...patch } : n)));
  }

  function set_link(i: number, patch: Partial<LinkRow>) {
    setLinks(links.map((l, j) => (j === i ? { ...l, ...patch } : l)));
  }

  async function save(e: React.FormEvent) {
    e.preventDefault();
    setStatus("");
    const trimmed = nodes.map((n) => ({ ...n, id: n.id.trim() }));
    if (trimmed.some((n) => !n.id)) {
      setError("node ids must be non-empty");
      return;
    }
    const ids = trimmed.map((n) => n.id);
    if (new Set(ids).size !== ids.length) {
      setError("node ids must be unique");
      return;
    }
    let parsed: { id: string; stage: string; params: unknown }[];
    try {
      parsed = trimmed.map((n) => ({ id: n.id, stage: n.stage, params: JSON.parse(n.params || "{}") }));
    } catch {
      setError("node params must be valid JSON");
      return;
    }
    if (links.some((l) => !l.from || !l.to)) {
      setError("link endpoints must be chosen");
      return;
    }
    const spec = { nodes: parsed, links: links.map((l) => ({ from: l.from, to: l.to })) };
    setError("");
    try {
      if (selected === "new") {
        const created = await create_pipeline(name.trim(), spec);
        setSelected(created.id);
        setName(created.name);
      } else {
        await update_pipeline(selected as number, name.trim(), spec);
      }
      setStatus("saved");
      await load_pipelines();
    } catch (err) {
      handle(err);
    }
  }

  async function del() {
    if (selected == null || selected === "new") return;
    setError("");
    try {
      await remove_pipeline(selected);
      setSelected(null);
      setName("");
      setNodes([]);
      setLinks([]);
      await load_pipelines();
    } catch (err) {
      handle(err);
    }
  }

  const nodeIds = nodes.map((n) => n.id);

  return (
    <main className="chat kanban-page">
      <header>
        <h1><Link to="/pipelines">pipelines</Link></h1>
        <span className="sub">{pipelines.length} saved</span>
        <nav className="nav">
          <Link to="/chat" title="back to chat"><ArrowLeft size={16} /></Link>
        </nav>
      </header>
      <div className="kanban-add">
        <select
          className="kanban-select"
          value={selected ?? ""}
          onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
            const p = pipelines.find((x) => x.id === Number(e.target.value));
            if (p) pick(p);
          }}
          title="saved pipeline"
        >
          <option value="">no pipeline selected</option>
          {pipelines.map((p) => (
            <option key={p.id} value={p.id}>{p.name}</option>
          ))}
        </select>
        <button className="kanban-mini" onClick={start_new} title="new pipeline"><Plus size={13} /> new pipeline</button>
        <button className="kanban-mini" onClick={del} disabled={selected == null || selected === "new"} title="delete pipeline"><Trash2 size={13} /></button>
      </div>
      {selected != null && (
        <form className="settings-form pipeline-editor" onSubmit={save}>
          <label>name</label>
          <input value={name} onChange={(e) => setName(e.target.value)} placeholder="pipeline name" />
          <label>nodes</label>
          {nodes.map((n, i) => (
            <div className="pipeline-row" key={i}>
              <input
                className="pipeline-node-id"
                value={n.id}
                onChange={(e) => set_node(i, { id: e.target.value })}
                placeholder="node id"
              />
              <select value={n.stage} onChange={(e) => set_node(i, { stage: e.target.value as Stage })}>
                {STAGES.map((s) => (
                  <option key={s} value={s}>{s}</option>
                ))}
              </select>
              <textarea
                className="pipeline-params"
                value={n.params}
                onChange={(e) => set_node(i, { params: e.target.value })}
                rows={2}
                spellCheck={false}
                placeholder="{}"
              />
              <button type="button" className="kanban-mini" onClick={() => setNodes(nodes.filter((_, j) => j !== i))} title="remove node">
                <Trash2 size={13} />
              </button>
            </div>
          ))}
          <button
            type="button"
            className="kanban-mini pipeline-add"
            onClick={() => setNodes([...nodes, { id: "", stage: "custom", params: "{}" }])}
          >
            <Plus size={13} /> add node
          </button>
          <label>links</label>
          {links.map((l, i) => (
            <div className="pipeline-row" key={i}>
              <select value={l.from} onChange={(e) => set_link(i, { from: e.target.value })}>
                <option value="">from…</option>
                {nodeIds.map((id, j) => (
                  <option key={j} value={id}>{id}</option>
                ))}
              </select>
              <select value={l.to} onChange={(e) => set_link(i, { to: e.target.value })}>
                <option value="">to…</option>
                {nodeIds.map((id, j) => (
                  <option key={j} value={id}>{id}</option>
                ))}
              </select>
              <button type="button" className="kanban-mini" onClick={() => setLinks(links.filter((_, j) => j !== i))} title="remove link">
                <Trash2 size={13} />
              </button>
            </div>
          ))}
          <button type="button" className="kanban-mini pipeline-add" onClick={() => setLinks([...links, { from: "", to: "" }])}>
            <Plus size={13} /> add link
          </button>
          <button type="submit" disabled={!name.trim()}>save pipeline</button>
          {status && <span className="saved-mark">{status}</span>}
        </form>
      )}
    </main>
  );
}
