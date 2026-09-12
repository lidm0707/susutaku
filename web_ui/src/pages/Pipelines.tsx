import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
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
  type FinalConnectionState,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import {
  Bot,
  Globe,
  Image,
  Package,
  Play,
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
  upload_attachment,
  fetch_pipeline_schema,
  test_pipeline,
  type Pipeline,
  type PipelineSpec,
  type PipelineSchema,
  type PipelineRunRecord,
  type PortKind,
} from "../lib.js";
import { PromptModal } from "../ui/Overlay.js";
import { toast } from "../ui/Toast.js";
import { set_focus } from "../components/focus.js";

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

// palette = stages wired to a runtime engine (mirrors backend WIRED_STAGES);
// legacy stages (old saved specs) stay parseable and are flagged in the UI
const BASIC_NODES: { stage: Stage; label: string; desc: string; Icon: typeof Globe }[] = [
  { stage: "fetch", label: "fetch", desc: "download a url", Icon: Globe },
  { stage: "search", label: "search", desc: "search the web", Icon: Search },
  { stage: "ref_image", label: "reference image", desc: "attach an image", Icon: Image },
  { stage: "agent", label: "agent", desc: "pick which agent runs", Icon: Bot },
  { stage: "transform", label: "transform", desc: "upper / lower / trim", Icon: Workflow },
  { stage: "output_resource", label: "output", desc: "save the result", Icon: Package },
];
const BASIC_STAGES = new Set(BASIC_NODES.map((n) => n.stage));

const STAGE_ICONS: Record<string, typeof Globe> = Object.fromEntries(
  BASIC_NODES.map(({ stage, Icon }) => [stage, Icon])
);

/// While a connection drag is live: ids of nodes that accept the source's
/// output. Nodes outside the set paint their input handle red.
const ConnectCtx = createContext<Set<string> | null>(null);

const DEBOUNCE_MS = 800;
const SAVING_MARK = "saving…";
const SAVED_MARK = "saved";

/// CSS handle class per port kind, so the user sees what connects to what.
const PORT_CLASS: Record<PortKind, string> = {
  any: "port-any",
  text: "port-text",
  json: "port-json",
  image: "port-image",
};

function ports_compatible(schema: PipelineSchema, out: string, inp: string): boolean {
  const src = schema[out];
  const dst = schema[inp];
  if (!src || !dst) return true;
  return src.output === "any" || dst.input === "any" || src.output === dst.input;
}

const NODE_X_STEP = 280;
const NODE_Y_STEP = 110;
const NODE_ORIGIN_X = 60;
const NODE_ORIGIN_Y = 40;
/// px radius around a handle where a drop still connects (n8n-style magnet).
const DOCK_DROP_RADIUS = 32;

type FlowData = {
  stage: Stage;
  params: string;
  input?: PortKind;
  output?: PortKind;
  run?: RunStatusKind;
};

type FlowNode = Node<FlowData>;

/// Per-node run status from the latest test run: undefined = not run.
type RunStatusKind = "ok" | "failed";
const run_by_node = (run: PipelineRunRecord | null): Map<string, RunStatusKind> => {
  const map = new Map<string, RunStatusKind>();
  if (run) for (const s of run.stages) map.set(s.node, s.status);
  return map;
};

function StageNode({ id, data, selected }: NodeProps<FlowNode>) {
  const Icon = STAGE_ICONS[data.stage] ?? Workflow;
  const valid = useContext(ConnectCtx);
  const bad_target = valid != null && !valid.has(id);
  const unwired = SCHEMA[data.stage] && SCHEMA[data.stage].wired === false;
  const cls = [
    selected ? "pipe-node selected" : "pipe-node",
    unwired ? "pipe-node-unwired" : "",
    data.run ? `pipe-node-${data.run}` : "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={cls} title={SCHEMA[data.stage]?.doc ?? data.stage}>
      <Handle
        type="target"
        position={Position.Top}
        className={`${data.input ? PORT_CLASS[data.input] : ""}${bad_target ? " handle-invalid" : ""}`}
      />
      <span className="pipe-node-stage">
        <Icon size={12} />
        {data.stage}
        {unwired && <em className="pipe-node-flag">not wired</em>}
      </span>
      <span className="pipe-node-io">
        in {data.input ?? "any"} → out {data.output ?? "any"}
      </span>
      <span className="pipe-node-params">{data.params === "{}" ? "no params" : data.params}</span>
      <Handle
        type="source"
        position={Position.Bottom}
        className={data.output ? PORT_CLASS[data.output] : ""}
      />
    </div>
  );
}

const NODE_TYPES = { stage: StageNode };

/// Ports per stage, from the backend schema (fetched once at module load).
let SCHEMA: PipelineSchema = {};

function stage_ports(stage: string): { input?: PortKind; output?: PortKind } {
  const p = SCHEMA[stage];
  return p ? { input: p.input, output: p.output } : {};
}

function layout(spec: PipelineSpec, run: PipelineRunRecord | null): { nodes: FlowNode[]; edges: Edge[] } {
  const ns = spec.nodes ?? [];
  const ls = spec.links ?? [];
  const status = run_by_node(run);
  // stored positions win; auto-place only nodes saved without x/y
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
    const auto_x = NODE_ORIGIN_X + d * NODE_X_STEP;
    const auto_y = NODE_ORIGIN_Y + row * NODE_Y_STEP;
    return {
      id: n.id,
      type: "stage",
      position: {
        x: n.x ?? auto_x,
        y: n.y ?? auto_y,
      },
      data: {
        stage: (n.stage as Stage) || "custom",
        params: JSON.stringify(n.params ?? {}, null, 2),
        run: status.get(n.id),
        ...stage_ports(n.stage),
      },
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

/// Param inputs keep local state so canvas re-renders never reset them;
/// the key (node id + param) remounts it only when the edited node changes.
function ParamField({
  id,
  param_key,
  hint,
  required,
  value,
  on_change,
}: {
  id: string;
  param_key: string;
  hint: string;
  required: boolean;
  value: string;
  on_change: (key: string, value: string) => void;
}) {
  const [local, setLocal] = useState(value);
  return (
    <div className="stage-field">
      <label>
        {param_key}
        {required ? " *" : ""}
      </label>
      <input
        key={`${id}:${param_key}`}
        value={local}
        onChange={(e) => {
          setLocal(e.target.value);
          on_change(param_key, e.target.value);
        }}
        placeholder={hint}
        spellCheck={false}
      />
    </div>
  );
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
      return { id: n.id, stage: n.data.stage, params, x: n.position.x, y: n.position.y };
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
  const [, setError] = useState("");
  const [status, setStatus] = useState("");
  const [newOpen, setNewOpen] = useState(false);
  const [testOpen, setTestOpen] = useState(false);
  const [run, setRun] = useState<PipelineRunRecord | null>(null);
  const [, setSchemaTick] = useState(0);
  const dragRef = useRef(false);
  // source node of a connection drag released on empty canvas (n8n-style);
  // a drop menu at the cursor spawns that stage already linked to the source
  const [pending, setPending] = useState<{ source: string; x: number; y: number } | null>(null);
  // source node id while a connection drag is live (for red invalid handles)
  const [connectFrom, setConnectFrom] = useState<string | null>(null);
  const canvasRef = useRef<HTMLDivElement | null>(null);
  const selectedRef = useRef<number | "new" | null>(selected);
  selectedRef.current = selected;
  const saveTimer = useRef<number | null>(null);
  const dirtyRef = useRef(false);
  const lastSpecRef = useRef("");

  useEffect(() => {
    load_pipelines();
    fetch_pipeline_schema()
      .then((s) => {
        SCHEMA = s;
        setSchemaTick((t) => t + 1);
      })
      .catch(() => {});
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
      const rows = await fetch_pipelines();
      setPipelines(rows);
      if (selectedRef.current == null && rows.length > 0) pick(rows[0]);
    } catch (err) {
      handle(err);
    }
  }

  useEffect(() => {
    fetch_agents()
      .then((rows) => setAgentNames(rows.filter((a) => a.id !== "new").map((a) => a.name)))
      .catch(() => setAgentNames([]));
  }, []);

  function load_flow(p: Pipeline, run_record: PipelineRunRecord | null = run) {
    const { nodes: ns, edges: es } = layout(p.spec || { nodes: [], links: [] }, run_record);
    setNodes(ns);
    setEdges(es);
    return { ns, es };
  }

  function pick(p: Pipeline) {
    setError("");
    setStatus("");
    setRun(null);
    setSelected(p.id);
    set_focus({ kind: "pipeline", id: p.id, name: p.name });
    setName(p.name);
    setEditId(null);
    const { ns, es } = load_flow(p);
    lastSpecRef.current = JSON.stringify({ name: p.name.trim(), spec: to_spec(ns, es) });
  }

  async function start_new(pipeline_name: string) {
    setError("");
    setStatus("");
    setEditId(null);
    try {
      const created = await create_pipeline(pipeline_name, { nodes: [], links: [] });
      setSelected(created.id);
      setName(created.name);
      setNodes([]);
      setEdges([]);
      lastSpecRef.current = JSON.stringify({ name: created.name.trim(), spec: { nodes: [], links: [] } });
      await load_pipelines();
      toast(`pipeline "${pipeline_name}" created`, "success");
    } catch (err) {
      handle(err);
    }
  }

  function spawn_node(stage: Stage = "fetch", connect_from?: string) {
    const taken = new Set(nodes.map((n) => n.id));
    let i = nodes.length + 1;
    while (taken.has(`node-${i}`)) i++;
    const id = `node-${i}`;
    const src_node = connect_from ? nodes.find((n) => n.id === connect_from) : undefined;
    const row = nodes.filter((n) => n.position.x === NODE_ORIGIN_X).length;
    const node: FlowNode = {
      id,
      type: "stage",
      position: {
        x: src_node ? src_node.position.x + NODE_X_STEP : NODE_ORIGIN_X,
        y: src_node ? src_node.position.y : NODE_ORIGIN_Y + row * NODE_Y_STEP,
      },
      data: { stage, params: "{}", ...stage_ports(stage) },
    };
    setNodes([...nodes, node]);
    if (src_node) {
      setEdges(
        addEdge(
          { source: src_node.id, target: id, animated: true, id: `e-${src_node.id}-${id}-new` },
          edges
        )
      );
    }
    setEditId(id);
    setEditStage(stage);
    setEditParams("{}");
  }

  const on_connect = useCallback(
    (c: Connection) => {
      setPending(null);
      setConnectFrom(null);
      setEdges((es) =>
        addEdge({ ...c, animated: true, id: `e-${c.source}-${c.target}-${es.length}` }, es)
      );
    },
    [setEdges]
  );

  const on_connect_start = useCallback(
    (_: MouseEvent | TouchEvent, { nodeId, handleType }: { nodeId: string | null; handleType: string | null }) => {
      if (nodeId && handleType === "source") setConnectFrom(nodeId);
    },
    []
  );

  // n8n pattern: drop a source-handle drag on empty canvas and a menu at the
  // cursor offers the next node, spawned already linked to the source.
  const on_connect_end = useCallback(
    (event: MouseEvent | TouchEvent, state: FinalConnectionState) => {
      setConnectFrom(null);
      if (!state.fromNode || state.isValid) return;
      const pt = event instanceof MouseEvent ? { x: event.clientX, y: event.clientY } : { x: event.touches[0]?.clientX ?? 0, y: event.touches[0]?.clientY ?? 0 };
      const rect = canvasRef.current?.getBoundingClientRect();
      if (!rect) return;
      setPending({ source: state.fromNode.id, x: pt.x - rect.left, y: pt.y - rect.top });
    },
    []
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
    // a stage change can invalidate existing links: drop those, so the spec
    // never keeps wiring the run would reject
    if (use_stage !== node.data.stage) {
      const keep = (e: Edge) => {
        if (e.source !== editId && e.target !== editId) return true;
        const other = nodes.find((n) => n.id === (e.source === editId ? e.target : e.source));
        if (!other) return false;
        return e.source === editId
          ? ports_compatible(SCHEMA, use_stage, other.data.stage)
          : ports_compatible(SCHEMA, other.data.stage, use_stage);
      };
      setEdges(edges.filter(keep));
    }
    setNodes((ns) =>
      ns.map((n) =>
        n.id === editId
          ? {
              ...n,
              id,
              data: {
                ...n.data,
                stage: use_stage,
                params: use_params,
                ...stage_ports(use_stage),
              },
            }
          : n
      )
    );
    if (id !== editId) {
      setEdges((es) =>
        es.map((e) => ({
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

  async function upload_ref_image(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file) return;
    try {
      const up = await upload_attachment(file);
      edit_param("path", up.path);
      setStatus("uploaded");
    } catch (err) {
      handle(err);
    }
  }

  async function do_save(mark: boolean) {
    if (selected == null || selected === "new") return;
    const spec = to_spec(nodes as FlowNode[], edges);
    try {
      await update_pipeline(selected, name.trim(), spec);
      lastSpecRef.current = JSON.stringify({ name: name.trim(), spec });
      dirtyRef.current = false;
      if (mark) setStatus(SAVED_MARK);
    } catch (err) {
      handle(err);
      if (mark) setStatus("");
    }
  }

  async function save() {
    setStatus(SAVING_MARK);
    if (saveTimer.current != null) {
      window.clearTimeout(saveTimer.current);
      saveTimer.current = null;
    }
    await do_save(true);
    await load_pipelines();
  }

  function schedule_save() {
    if (saveTimer.current != null) window.clearTimeout(saveTimer.current);
    setStatus(SAVING_MARK);
    saveTimer.current = window.setTimeout(() => {
      saveTimer.current = null;
      void do_save(true);
    }, DEBOUNCE_MS);
  }

  const snapshot = useCallback(
    () => JSON.stringify({ name: name.trim(), spec: to_spec(nodes as FlowNode[], edges) }),
    [name, nodes, edges]
  );

  // autosave: any graph/name change schedules a debounced save; skipped mid-drag
  useEffect(() => {
    if (selected == null || selected === "new") return;
    const snap = snapshot();
    if (snap === lastSpecRef.current) return;
    dirtyRef.current = true;
    if (dragRef.current) return;
    schedule_save();
  });

  // flush a pending save on unmount (e.g. navigating away mid-edit)
  const flushRef = useRef(do_save);
  flushRef.current = do_save;
  useEffect(
    () => () => {
      if (saveTimer.current != null) window.clearTimeout(saveTimer.current);
      if (dirtyRef.current) void flushRef.current(false);
    },
    []
  );

  async function del() {
    if (selected == null || selected === "new") return;
    setError("");
    try {
      await remove_pipeline(selected);
      setSelected(null);
      set_focus(null);
      await load_pipelines();
    } catch (err) {
      handle(err);
    }
  }

  async function run_test(seed_text: string) {
    if (selected == null || selected === "new") return;
    setError("");
    try {
      await do_save(false);
      const record = await test_pipeline(selected, seed_text);
      setRun(record);
      const status_map = run_by_node(record);
      setNodes((ns) =>
        ns.map((n) => ({
          ...n,
          data: { ...n.data, run: status_map.get(n.id) },
        }))
      );
      toast(
        record.status === "ok" ? "pipeline test passed" : "pipeline test failed",
        record.status === "ok" ? "success" : "error"
      );
    } catch (err) {
      handle(err);
    }
  }

  const editor = useMemo(() => selected != null, [selected]);

  // node ids that accept the live connection drag's output (red when not in set)
  const validTargets = useMemo(() => {
    if (!connectFrom) return null;
    const src = nodes.find((n) => n.id === connectFrom);
    if (!src) return null;
    return new Set(
      nodes.filter((n) => n.id !== connectFrom && ports_compatible(SCHEMA, src.data.stage, n.data.stage)).map((n) => n.id)
    );
  }, [connectFrom, nodes]);

  return (
    <main className="chat kanban-page pipeline-page">
      <header>
        <h1>pipelines</h1>
        <span className="sub">{pipelines.length} saved</span>
      </header>
      <div className="pipeline-layout">
        <aside className="agents-side">
          <button className="agents-new" onClick={() => setNewOpen(true)}>
            <Plus size={14} /> new pipeline
          </button>
          <div className="agents-list">
            {pipelines.length === 0 && (
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
                onChange={(e) => setName(e.target.value)}
                spellCheck={false}
                aria-label="pipeline name"
              />
              <button className="kanban-mini" onClick={() => spawn_node()} title="add node">
                <Plus size={13} /> node
              </button>
              <button className="kanban-mini" onClick={() => setTestOpen(true)} disabled={selected == null || selected === "new"} title="test run with seed text">
                <Play size={13} /> test
              </button>
              <button className="kanban-mini" onClick={del} disabled={selected == null || selected === "new"} title="delete pipeline">
                <Trash2 size={13} />
              </button>
              <button type="button" className="pipeline-save" onClick={save} disabled={selected == null || selected === "new"} title="save pipeline">
                <Save size={13} /> save pipeline
              </button>
              {status && <span className="saved-mark">{status}</span>}
            </div>
            <div className="pipeline-canvas" ref={canvasRef}>
              <ConnectCtx.Provider value={validTargets}>
                <ReactFlow
                  nodes={nodes}
                  edges={edges}
                  onNodesChange={onNodesChange}
                  onEdgesChange={onEdgesChange}
                  onConnect={on_connect}
                  onConnectStart={on_connect_start}
                  onConnectEnd={on_connect_end}
                  connectionRadius={DOCK_DROP_RADIUS}
                isValidConnection={(c) => {
                  const src = nodes.find((n) => n.id === c.source);
                  const dst = nodes.find((n) => n.id === c.target);
                  if (!src || !dst) return false;
                  return ports_compatible(SCHEMA, src.data.stage, dst.data.stage);
                }}
                nodeTypes={NODE_TYPES}
                onNodeClick={(_, n) => select_node(n as FlowNode)}
                onPaneClick={() => {
                  setPending(null);
                  select_node(null);
                }}
                onNodeDragStart={() => {
                  dragRef.current = true;
                }}
                onNodeDragStop={(_, n) => {
                  dragRef.current = false;
                  select_node(n as FlowNode);
                  if (snapshot() !== lastSpecRef.current) schedule_save();
                }}
                fitView
                proOptions={{ hideAttribution: true }}
                deleteKeyCode={["Backspace", "Delete"]}
              />
            </ConnectCtx.Provider>
              {pending && (
                <div className="pipe-drop-menu" style={{ left: pending.x, top: pending.y }} role="menu">
                  <span className="pipe-drop-title">add node</span>
                  {BASIC_NODES.map(({ stage, label, desc, Icon }) => (
                    <button
                      key={stage}
                      type="button"
                      role="menuitem"
                      title={SCHEMA[stage]?.doc ?? desc}
                      onClick={() => {
                        const src = pending.source;
                        setPending(null);
                        spawn_node(stage, src);
                      }}
                    >
                      <Icon size={13} />
                      {label}
                      <span>{desc}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>
            {editId && (
              <div className="pipeline-inspector">
                <div className="inspector-head">
                  <span>
                    {(() => {
                      const basic = BASIC_NODES.find((b) => b.stage === editStage);
                      const Icon = basic?.Icon ?? Workflow;
                      return (
                        <>
                          <Icon size={13} />
                          {basic?.label ?? `node settings (${editStage})`}
                        </>
                      );
                    })()}
                  </span>
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
                    <span className="stage-hint">legacy node ({editStage}) — not wired to an engine, switch to a wired type:</span>
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
                    <span className="stage-hint">{SCHEMA[editStage]?.doc ?? ""}</span>
                    <div className="stage-io">
                      <span>in: {SCHEMA[editStage]?.input ?? "any"}</span>
                      <span>out: {SCHEMA[editStage]?.output ?? "any"}</span>
                    </div>
                    {(SCHEMA[editStage]?.params ?? []).map(({ key, hint, required }) =>
                      editStage === "ref_image" && key === "path" ? (
                        <div key={key} className="stage-field">
                          <label>image</label>
                          <input type="file" accept="image/*" onChange={upload_ref_image} />
                          {String(parse_params(editParams).path ?? "") && (
                            <span className="stage-path">{String(parse_params(editParams).path)}</span>
                          )}
                        </div>
                      ) : editStage === "agent" && key === "agent" && agentNames.length > 0 ? (
                        <div key={key} className="stage-field">
                          <label>{key}{required ? " *" : ""}</label>
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
                        <ParamField
                          key={`${editId}:${key}`}
                          id={editId}
                          param_key={key}
                          hint={hint}
                          value={String(parse_params(editParams)[key] ?? "")}
                          on_change={edit_param}
                          required={required}
                        />
                      )
                    )}
                  </>
                )}
                {run && editId && (() => {
                  const st = run.stages.find((s) => s.node === editId);
                  return st ? (
                    <div className={st.status === "ok" ? "run-note ok" : "run-note failed"}>
                      <strong>{st.status}</strong> {st.note}
                    </div>
                  ) : null;
                })()}
                <button type="button" className="pipeline-inspector-del" onClick={remove_node}>
                  <Trash2 size={13} /> remove node
                </button>
              </div>
            )}
            {!editId && !run && (
              <div className="pipeline-hint">
                click a node to edit · drag bottom dot to top dot to link · drop on empty canvas
                to pick the next node · invalid targets show a red dot · click edge + ⌫ to unlink
              </div>
            )}
            <div className="pipeline-dock">
              {BASIC_NODES.map(({ stage, label, desc, Icon }) => (
                <button
                  key={stage}
                  type="button"
                  className="pipeline-dock-btn"
                  title={`${label} — ${SCHEMA[stage]?.doc ?? desc}`}
                  aria-label={`add ${label} node`}
                  onClick={() => spawn_node(stage)}
                >
                  <Icon size={15} />
                </button>
              ))}
            </div>
            {run && !editId && (
              <div className="pipeline-run">
                <div className="pipeline-run-head">
                  <span className={run.status === "ok" ? "run-note ok" : "run-note failed"}>
                    test {run.status} · {run.stages.filter((s) => s.status === "ok").length}/{run.stages.length} nodes
                  </span>
                  <button type="button" className="icon-btn" onClick={() => { setRun(null); load_flow(pipelines.find((p) => p.id === selected)!, null); }} title="clear run">
                    <X size={14} />
                  </button>
                </div>
                {run.output && <pre className="pipeline-run-output">{run.output}</pre>}
              </div>
            )}
          </div>
        ) : (
          <div className="agent-editor agent-editor-empty">
            <Workflow size={28} />
            <p>select a pipeline on the left,<br />or create a new one.</p>
            <button onClick={() => setNewOpen(true)}><Plus size={14} /> new pipeline</button>
          </div>
        )}
      </div>
      <PromptModal
        open={newOpen}
        title="new pipeline"
        placeholder="name…"
        on_close={() => setNewOpen(false)}
        on_submit={(v) => {
          setNewOpen(false);
          start_new(v);
        }}
      />
      <PromptModal
        open={testOpen}
        title="test pipeline"
        placeholder="seed text…"
        on_close={() => setTestOpen(false)}
        on_submit={(v) => {
          setTestOpen(false);
          run_test(v);
        }}
      />
    </main>
  );
}
