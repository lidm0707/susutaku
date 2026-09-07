import { useCallback, useEffect, useMemo, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import {
  ReactFlow,
  Handle,
  Position,
  addEdge,
  useNodesState,
  useEdgesState,
  type Connection,
  type Edge,
  type Node,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { ArrowLeft, Plus, Save, Trash2, Workflow } from "lucide-react";
import {
  clear_token,
  create_pipeline,
  fetch_pipelines,
  remove_pipeline,
  update_pipeline,
  type Pipeline,
  type PipelineSpec,
} from "../lib.js";

const STAGES = ["ingest", "parse", "transform", "model_infer", "render", "custom"] as const;

type Stage = (typeof STAGES)[number];

const NODE_X_STEP = 280;
const NODE_Y_STEP = 110;
const NODE_ORIGIN_X = 60;
const NODE_ORIGIN_Y = 40;

type FlowData = { stage: Stage; params: string };

type FlowNode = Node<FlowData>;

function StageNode({ data, selected }: NodeProps<FlowNode>) {
  return (
    <div className={selected ? "pipe-node selected" : "pipe-node"}>
      <Handle type="target" position={Position.Top} />
      <span className="pipe-node-stage">{data.stage}</span>
      <span className="pipe-node-params">{data.params === "{}" ? "no params" : data.params}</span>
      <Handle type="source" position={Position.Bottom} />
    </div>
  );
}

const NODE_TYPES = { stage: StageNode };

function layout(spec: PipelineSpec): { nodes: FlowNode[]; edges: Edge[] } {
  const ns = spec.nodes ?? [];
  const ls = spec.links ?? [];
  const depth = new Map(ns.map((n) => [n.id, 0]));
  for (let pass = 0; pass < ns.length; pass++) {
    for (const l of ls) {
      const d = (depth.get(l.from) ?? 0) + 1;
      if ((depth.get(l.to) ?? 0) < d) depth.set(l.to, d);
    }
  }
  const perDepth = new Map<number, number>();
  const nodes: FlowNode[] = ns.map((n) => {
    const d = depth.get(n.id) ?? 0;
    const row = perDepth.get(d) ?? 0;
    perDepth.set(d, row + 1);
    return {
      id: n.id,
      type: "stage",
      position: { x: NODE_ORIGIN_X + d * NODE_X_STEP, y: NODE_ORIGIN_Y + row * NODE_Y_STEP },
      data: { stage: (n.stage as Stage) || "custom", params: JSON.stringify(n.params ?? {}, null, 2) },
    };
  });
  const edges: Edge[] = ls
    .filter((l) => ns.some((n) => n.id === l.from) && ns.some((n) => n.id === l.to))
    .map((l, i) => ({
      id: `e-${l.from}-${l.to}-${i}`,
      source: l.from,
      target: l.to,
      animated: true,
    }));
  return { nodes, edges };
}

function to_spec(nodes: FlowNode[], edges: Edge[]): PipelineSpec {
  return {
    nodes: nodes.map((n) => {
      let params: unknown;
      try {
        params = JSON.parse(n.data.params || "{}");
      } catch {
        params = {};
      }
      return { id: n.id, stage: n.data.stage, params };
    }),
    links: edges
      .filter((e) => nodes.some((n) => n.id === e.source) && nodes.some((n) => n.id === e.target))
      .map((e) => ({ from: e.source, to: e.target })),
  };
}

export default function Pipelines() {
  const nav = useNavigate();
  const [pipelines, setPipelines] = useState<Pipeline[]>([]);
  const [selected, setSelected] = useState<number | "new" | null>(null);
  const [name, setName] = useState("");
  const [nodes, setNodes, onNodesChange] = useNodesState<FlowNode>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);
  const [editId, setEditId] = useState<string | null>(null);
  const [editStage, setEditStage] = useState<Stage>("custom");
  const [editParams, setEditParams] = useState("{}");
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

  function load_flow(p: Pipeline) {
    const { nodes: ns, edges: es } = layout(p.spec || { nodes: [], links: [] });
    setNodes(ns);
    setEdges(es);
  }

  function pick(p: Pipeline) {
    setError("");
    setStatus("");
    setSelected(p.id);
    setName(p.name);
    setEditId(null);
    load_flow(p);
  }

  function start_new() {
    setError("");
    setStatus("");
    setSelected("new");
    setName("");
    setEditId(null);
    setNodes([]);
    setEdges([]);
  }

  function spawn_node() {
    const taken = new Set(nodes.map((n) => n.id));
    let i = nodes.length + 1;
    while (taken.has(`node-${i}`)) i++;
    const id = `node-${i}`;
    const row = nodes.filter((n) => n.position.x === NODE_ORIGIN_X).length;
    setNodes([
      ...nodes,
      {
        id,
        type: "stage",
        position: { x: NODE_ORIGIN_X, y: NODE_ORIGIN_Y + row * NODE_Y_STEP },
        data: { stage: "custom", params: "{}" },
      },
    ]);
    setEditId(id);
    setEditStage("custom");
    setEditParams("{}");
  }

  const on_connect = useCallback(
    (c: Connection) =>
      setEdges((es) =>
        addEdge({ ...c, animated: true, id: `e-${c.source}-${c.target}-${es.length}` }, es)
      ),
    [setEdges]
  );

  function select_node(n: FlowNode | null) {
    if (!n) {
      setEditId(null);
      return;
    }
    setEditId(n.id);
    setEditStage(n.data.stage);
    setEditParams(n.data.params);
  }

  function apply_edit(stage?: Stage, params?: string) {
    if (!editId) return;
    const node = nodes.find((n) => n.id === editId);
    if (!node) return;
    const id = editId.trim();
    const use_stage = stage ?? editStage;
    const use_params = params ?? editParams;
    if (!id) {
      setError("node id must be non-empty");
      return;
    }
    if (nodes.some((n) => n.id === id && n.id !== editId)) {
      setError("node ids must be unique");
      return;
    }
    setNodes(
      nodes.map((n) =>
        n.id === editId ? { ...n, id, data: { ...n.data, stage: use_stage, params: use_params } } : n
      )
    );
    if (id !== editId) {
      setEdges(
        edges.map((e) => ({
          ...e,
          source: e.source === editId ? id : e.source,
          target: e.target === editId ? id : e.target,
        }))
      );
    }
    setStatus("");
  }

  function remove_node() {
    if (!editId) return;
    setNodes(nodes.filter((n) => n.id !== editId));
    setEdges(edges.filter((e) => e.source !== editId && e.target !== editId));
    setEditId(null);
  }

  async function save(e: React.FormEvent | React.MouseEvent) {
    e.preventDefault();
    setStatus("");
    if (!name.trim()) return;
    const spec = to_spec(nodes as FlowNode[], edges);
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
      start_new();
      await load_pipelines();
    } catch (err) {
      handle(err);
    }
  }

  const editor = useMemo(() => selected != null, [selected]);

  return (
    <main className="chat kanban-page pipeline-page">
      <header>
        <h1><Link to="/pipelines">pipelines</Link></h1>
        <span className="sub">{pipelines.length} saved</span>
        <nav className="nav">
          <Link to="/chat" title="back to chat"><ArrowLeft size={16} /></Link>
        </nav>
      </header>
      <div className="pipeline-layout">
        <aside className="agents-side">
          <button className="agents-new" onClick={start_new}>
            <Plus size={14} /> new pipeline
          </button>
          <div className="agents-list">
            {pipelines.length === 0 && <span className="agents-empty">no pipelines yet</span>}
            {pipelines.map((p) => (
              <button
                key={p.id}
                className={selected === p.id ? "agent-item active" : "agent-item"}
                onClick={() => pick(p)}
              >
                <Workflow size={14} />
                <span className="agent-item-name">{p.name}</span>
              </button>
            ))}
          </div>
        </aside>
        {editor ? (
          <div className="pipeline-main">
            <div className="pipeline-toolbar">
              <input
                className="pipeline-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="pipeline name"
              />
              <button className="kanban-mini" onClick={spawn_node} title="add node">
                <Plus size={13} /> node
              </button>
              <button className="kanban-mini" onClick={del} disabled={selected == null || selected === "new"} title="delete pipeline">
                <Trash2 size={13} />
              </button>
              <button className="pipeline-save" onClick={save} disabled={!name.trim()}>
                <Save size={13} /> save
              </button>
              {status && <span className="saved-mark">{status}</span>}
            </div>
            <div className="pipeline-canvas">
              <ReactFlow
                nodes={nodes}
                edges={edges}
                onNodesChange={onNodesChange}
                onEdgesChange={onEdgesChange}
                onConnect={on_connect}
                nodeTypes={NODE_TYPES}
                onNodeClick={(_, n) => select_node(n as FlowNode)}
                onPaneClick={() => select_node(null)}
                onNodeDragStop={(_, n) => select_node(n as FlowNode)}
                fitView
                proOptions={{ hideAttribution: true }}
                deleteKeyCode={["Backspace", "Delete"]}
              />
            </div>
            {editId && (
              <div className="pipeline-inspector">
                <label>node id</label>
                <input
                  value={editId}
                  onChange={(e) => setEditId(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && apply_edit()}
                  onBlur={() => apply_edit()}
                />
                <label>stage</label>
                <select value={editStage} onChange={(e) => { const s = e.target.value as Stage; setEditStage(s); apply_edit(s); }}>
                  {STAGES.map((s) => (
                    <option key={s} value={s}>{s}</option>
                  ))}
                </select>
                <label>params (json)</label>
                <textarea
                  value={editParams}
                  onChange={(e) => setEditParams(e.target.value)}
                  onBlur={() => apply_edit(undefined, editParams)}
                  rows={5}
                  spellCheck={false}
                />
                <button type="button" className="pipeline-inspector-del" onClick={remove_node}>
                  <Trash2 size={13} /> remove node
                </button>
              </div>
            )}
            {!editId && (
              <div className="pipeline-hint">
                click a node to edit · drag from bottom dot to top dot to link · del removes selected
              </div>
            )}
          </div>
        ) : (
          <div className="agent-editor agent-editor-empty">
            <Workflow size={28} />
            <p>select a pipeline on the left,<br />or create a new one.</p>
            <button onClick={start_new}><Plus size={14} /> new pipeline</button>
          </div>
        )}
      </div>
    </main>
  );
}
