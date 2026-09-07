import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
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
import {
  Bot,
  Globe,
  Image,
  Package,
  Plus,
  Save,
  Search,
  Trash2,
  Workflow,
  X,
} from "lucide-react";
import {
  clear_token,
  create_pipeline,
  fetch_pipelines,
  remove_pipeline,
  update_pipeline,
  fetch_agents,
  type Pipeline,
  type PipelineSpec,
} from "../lib.js";

// basic connector nodes a user can pick, then legacy stages (old saved specs only)
const STAGES = [
  "fetch",
  "search",
  "ref_image",
  "output_resource",
  "agent",
  "ingest",
  "parse",
  "transform",
  "model_infer",
  "render",
] as const;

type Stage = (typeof STAGES)[number];

/// Stage picker metadata: the 4 basic nodes with icon + one-liner.
const BASIC_NODES: {
  stage: Stage;
  label: string;
  desc: string;
  Icon: typeof Globe;
}[] = [
  { stage: "fetch", label: "fetch", desc: "download a url", Icon: Globe },
  { stage: "search", label: "search", desc: "search the web", Icon: Search },
  { stage: "ref_image", label: "reference image", desc: "attach an image", Icon: Image },
  { stage: "agent", label: "agent", desc: "pick which agent runs", Icon: Bot },
  { stage: "output_resource", label: "output", desc: "save the result", Icon: Package },
];
const BASIC_STAGES = new Set(BASIC_NODES.map((n) => n.stage));

/// Per-stage param fields + usage shown in the inspector.
const STAGE_FIELDS: Partial<Record<Stage, { key: string; hint: string }[]>> = {
  fetch: [
    { key: "url", hint: "http(s) url to fetch" },
    { key: "method", hint: "http method, default GET" },
  ],
  search: [{ key: "query", hint: "web search query" }],
  ref_image: [{ key: "path", hint: "image path or url to reference" }],
  agent: [{ key: "agent", hint: "agent name from Agents settings" }],
  output_resource: [{ key: "name", hint: "resource name to write result to" }],
};

/// How to use each node: what it does, what input it takes, what it emits.
const STAGE_DOCS: Record<Stage, string> = {
  fetch:
    "downloads the resource at params.url and emits the raw body as text downstream. " +
    "usually the first node; drag from its bottom dot into the next node's top dot.",
  search:
    "runs params.query as a web search and emits the results as text. " +
    "pair it with a model_infer/raw node to summarize the hits.",
  ref_image:
    "loads the image at params.path and attaches it to the payload for later nodes. " +
    "place it before a model_infer/raw node that can read the image.",
  output_resource:
    "writes the incoming payload to the resource named params.name. " +
    "use as the last node of a branch; it passes the payload through.",
  agent:
    "selects the configured agent (params.agent, from Agents settings) for the downstream " +
    "model_infer/raw nodes. place it before the node that calls the model.",
  ingest:
    "legacy: reads the input payload into the pipeline. kept for old saved specs.",
  parse:
    "legacy: parses raw text (e.g. json) into a structured payload. kept for old specs.",
  transform:
    "legacy: reshapes the payload fields. kept for old saved specs.",
  model_infer:
    "legacy: sends the payload to the selected model and emits the reply. kept for old specs.",
  render:
    "legacy: renders the payload to its final text form. kept for old specs.",
};

const NODE_X_STEP = 280;
const NODE_Y_STEP = 110;
const NODE_ORIGIN_X = 60;
const NODE_ORIGIN_Y = 40;

type FlowData = { stage: Stage; params: string };

type FlowNode = Node<FlowData>;

function StageNode({ data, selected }: NodeProps<FlowNode>) {
  return (
    <div className={selected ? "pipe-node selected" : "pipe-node"} title={STAGE_DOCS[data.stage]}>
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

function parse_params(s: string): Record<string, unknown> {
  try {
    const v = JSON.parse(s);
    return v && typeof v === "object" && !Array.isArray(v) ? v : {};
  } catch {
    return {};
  }
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
  const [editStage, setEditStage] = useState<Stage>("fetch");
  const [editParams, setEditParams] = useState("{}");
  const [agentNames, setAgentNames] = useState<string[]>([]);
  const [draftName, setDraftName] = useState<string | null>(null);
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

  useEffect(() => {
    fetch_agents()
      .then((rows) => setAgentNames(rows.filter((a) => a.id !== "new").map((a) => a.name)))
      .catch(() => setAgentNames([]));
  }, []);

  function load_flow(p: Pipeline) {
    const { nodes: ns, edges: es } = layout(p.spec || { nodes: [], links: [] });
    setNodes(ns);
    setEdges(es);
  }

  function pick(p: Pipeline) {
    setError("");
    setStatus("");
    setDraftName(null);
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
    setDraftName("");
    setEditId(null);
    setNodes([]);
    setEdges([]);
  }

  function spawn_node(stage: Stage = "fetch") {
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
        data: { stage, params: "{}" },
      },
    ]);
    setEditId(id);
    setEditStage(stage);
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

  function edit_param(key: string, value: string) {
    const p = parse_params(editParams);
    if (value === "") delete p[key];
    else p[key] = value;
    apply_edit(undefined, JSON.stringify(p, null, 2));
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
        setDraftName(null);
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
      setDraftName(null);
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
        <h1>pipelines</h1>
        <span className="sub">{pipelines.length} saved</span>
      </header>
      <div className="pipeline-layout">
        <aside className="agents-side">
          <button className="agents-new" onClick={start_new}>
            <Plus size={14} /> new pipeline
          </button>
          <div className="agents-list">
            {draftName !== null && (
              <div className={selected === "new" ? "agent-item draft active" : "agent-item draft"}>
                <Workflow size={14} />
                <input
                  className="agent-item-name draft-name"
                  value={draftName}
                  autoFocus
                  placeholder="untitled pipeline"
                  onChange={(e) => {
                    setDraftName(e.target.value);
                    setName(e.target.value);
                  }}
                />
              </div>
            )}
            {pipelines.length === 0 && draftName === null && (
              <span className="agents-empty">no pipelines yet</span>
            )}
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
                onChange={(e) => {
                  setName(e.target.value);
                  if (draftName !== null) setDraftName(e.target.value);
                }}
                placeholder="pipeline name"
              />
              <button className="kanban-mini" onClick={() => spawn_node()} title="add node">
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
                <div className="inspector-head">
                  <span>node settings</span>
                  <button type="button" className="icon-btn" onClick={() => setEditId(null)} title="close">
                    <X size={14} />
                  </button>
                </div>
                <div className="inspector-id">
                  <label>id</label>
                  <input
                    value={editId}
                    onChange={(e) => setEditId(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && apply_edit()}
                    onBlur={() => apply_edit()}
                    spellCheck={false}
                  />
                </div>
                {!BASIC_STAGES.has(editStage) ? (
                  <>
                    <span className="stage-hint">legacy node ({editStage}) — switch to a basic type:</span>
                    <div className="stage-picker">
                      {BASIC_NODES.map(({ stage, label, desc, Icon }) => (
                        <button key={stage} type="button" className="stage-card" onClick={() => { setEditStage(stage); apply_edit(stage); }}>
                          <Icon size={15} />
                          <span className="stage-card-label">{label}</span>
                          <span className="stage-card-desc">{desc}</span>
                        </button>
                      ))}
                    </div>
                  </>
                ) : (
                  <>
                    <div className="stage-picker">
                      {BASIC_NODES.map(({ stage, label, desc, Icon }) => (
                        <button
                          key={stage}
                          type="button"
                          className={stage === editStage ? "stage-card active" : "stage-card"}
                          onClick={() => { setEditStage(stage); apply_edit(stage); }}
                        >
                          <Icon size={15} />
                          <span className="stage-card-label">{label}</span>
                          <span className="stage-card-desc">{desc}</span>
                        </button>
                      ))}
                    </div>
                    <span className="stage-hint">{STAGE_DOCS[editStage]}</span>
                    {STAGE_FIELDS[editStage]!.map(({ key, hint }) =>
                      editStage === "agent" && key === "agent" && agentNames.length > 0 ? (
                        <div key={key} className="stage-field">
                          <label>{key}</label>
                          <select
                            value={String(parse_params(editParams)[key] ?? "")}
                            onChange={(e) => edit_param(key, e.target.value)}
                          >
                            <option value="">choose an agent…</option>
                            {agentNames.map((n) => (
                              <option key={n} value={n}>{n}</option>
                            ))}
                          </select>
                        </div>
                      ) : (
                        <div key={key} className="stage-field">
                          <label>{key}</label>
                          <input
                            value={String(parse_params(editParams)[key] ?? "")}
                            onChange={(e) => edit_param(key, e.target.value)}
                            placeholder={hint}
                            spellCheck={false}
                          />
                        </div>
                      )
                    )}
                  </>
                )}
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
