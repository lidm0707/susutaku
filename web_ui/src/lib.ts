import { toast } from "./ui/Toast.jsx";

export const API_BASE = import.meta.env.VITE_API_BASE || "";

const TOKEN_KEY = "susutaku_token";

export class ApiError extends Error {
  status: number;
  constructor(status: number, body: string) {
    super(`${status} ${body}`);
    this.status = status;
  }
}

export interface ModelInfo {
  name: string;
  engine: string;
  bytes: number;
  loadable: boolean;
  selected: boolean;
}

export interface CodexModel {
  id: string;
  label: string;
}

export interface ChatReply {
  reply?: string;
  model?: string;
  prompt_tps?: number;
  decode_tps?: number;
  searched?: boolean;
  tokenizer?: string;
  status?: string;
  [key: string]: unknown;
}

export interface Agent {
  id: number | "new";
  name: string;
  model: string;
  persona: string;
  prompt: string;
  output: string;
}

export interface Workspace {
  id: number;
  name: string;
}

export interface Project {
  id: number;
  name: string;
}

export interface Card {
  id: number;
  title: string;
  description: string;
  column_id: string;
  priority: string;
  agent_name?: string;
  agent_state?: unknown;
  assignee?: string | null;
  pipeline_id?: number | null;
  pipeline_name?: string;
  cron?: string | null;
  deadline?: string | null;
  labels: string | null;
  checklist: string | null;
  estimate: number | null;
}

export interface ChecklistItem {
  text: string;
  done: boolean;
}

export type RunStatus = "ok" | "failed";

export interface CardRunStage {
  node: string;
  stage: string;
  status: RunStatus;
  note: string;
}

export interface CardRun {
  pipeline_id: number;
  pipeline_name: string;
  status: RunStatus;
  stages: CardRunStage[];
  output?: string | null;
  finished_at: string;
}

export function run_of(card: Card): CardRun | null {
  if (!card.agent_state || typeof card.agent_state !== "object") return null;
  const run = (card.agent_state as Record<string, unknown>).run;
  return run && typeof run === "object" ? (run as CardRun) : null;
}

export interface CronJob {
  card_id: number;
  title: string;
  cron: string;
  pipeline_name?: string | null;
  next_run: number;
}

export interface Comment {
  id: number;
  card_id: number;
  author: string;
  body: string;
  created_at: string;
}

export interface PipelineNode {
  id: string;
  stage: string;
  params: unknown;
  x?: number;
  y?: number;
}

export interface PipelineLink {
  from: string;
  to: string;
}

export interface PipelineSpec {
  nodes: PipelineNode[];
  links: PipelineLink[];
}

export interface Pipeline {
  id: number;
  name: string;
  spec?: PipelineSpec;
}

export interface SandboxDir {
  pid: number;
  path: string;
  alive: boolean;
}

export interface ZaiModel {
  model: string;
  api_key_set: boolean;
}

export interface ZaiSettings {
  api_key_set: boolean;
  model: string;
  models: ZaiModel[];
  say_hi_time: string | null;
  say_hi_interval_mins: number | null;
  timezone: string | null;
}

export type ZaiModelAction =
  | { action: "add"; model: string; api_key: string }
  | { action: "set_key"; model: string; api_key: string }
  | { action: "remove"; model: string }
  | { action: "set_active"; model: string };

export interface ClientEnv {
  reported: boolean;
  user_agent: string;
  platform: string;
  language: string;
  timezone: string;
  screen: string;
  workspace_path: string;
  hostname: string;
  os: string;
  arch: string;
}

export type ClientEnvInput = Pick<ClientEnv, "user_agent" | "platform" | "language" | "timezone" | "screen" | "workspace_path">;

export async function fetch_client_env(): Promise<ClientEnv> {
  const res = await fetch(`${API_BASE}/api/settings/client-env`);
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function save_client_env(
  env: ClientEnvInput
): Promise<ClientEnv> {
  const res = await fetch(`${API_BASE}/api/settings/client-env`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(env),
  });
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export interface PromptSection {
  role: string;
  body: string;
}

export interface RenderedPrompt {
  rendered: string;
  chars: number;
  max_chars: number;
}

export interface UserInfo {
  username: string;
  role: string;
}

export function get_token(): string {
  return localStorage.getItem(TOKEN_KEY) || "";
}

export function save_token(token: string): void {
  localStorage.setItem(TOKEN_KEY, token);
}

export function clear_token(): void {
  localStorage.removeItem(TOKEN_KEY);
}

async function api(path: string, opts: RequestInit = {}): Promise<Response> {
  const token = get_token();
  const headers: Record<string, string> = { ...(opts.headers as Record<string, string> | undefined) };
  if (token) headers.Authorization = `Bearer ${token}`;
  const res = await fetch(`${API_BASE}${path}`, { ...opts, headers });
  if (!res.ok) {
    if (res.status === 401) {
      clear_token();
      window.dispatchEvent(new Event("susutaku:unauthorized"));
    }
    const err = new ApiError(res.status, await res.text());
    if (err.status !== 401) toast(err.message, "error");
    throw err;
  }
  return res;
}

export async function login(
  username: string,
  password: string
): Promise<{ token: string; username: string; role: string; must_change_password: boolean }> {
  const res = await api("/api/auth/login", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ username, password }),
  });
  const data = await res.json();
  save_token(data.token);
  return data;
}

export async function logout(): Promise<void> {
  try {
    await api("/api/auth/logout", { method: "POST" });
  } finally {
    clear_token();
  }
}

export async function fetch_bootstrap(): Promise<{ needs_setup: boolean }> {
  return (await api("/api/auth/bootstrap")).json();
}

export async function change_password(old_password: string, new_password: string): Promise<Response> {
  return api("/api/auth/change-password", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ old_password, new_password }),
  });
}

export async function register_user(
  username: string,
  password: string,
  role: string
): Promise<UserInfo> {
  const res = await api("/api/auth/users", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ username, password, role }),
  });
  return res.json();
}

export async function fetch_users(): Promise<UserInfo[]> {
  return (await api("/api/auth/users")).json();
}

const GIB = 1024 ** 3;

export function pretty_name(name: string): string {
  const S = "\u0001";
  return name
    .replace(/Q\d_[KLS](?:_M)?/g, (q) => q.replace(/_/g, S))
    .replace(/[-_]/g, " ")
    .replace(/(\d)b\b/g, "$1B")
    .replace(/\ba4b\b/i, "A4B")
    .replace(/\bit\b/i, "instruct")
    .replace(/(^|\s)q(?=\d)/gi, "$1Q")
    .replace(/\bgemma\b/i, "Gemma")
    .trim()
    .replace(new RegExp(S, "g"), "_");
}

export function size_label(bytes: number): string {
  return `${(bytes / GIB).toFixed(1)} GB`;
}

export function gib_label(bytes: number): string {
  return `${(bytes / GIB).toFixed(1)} GiB`;
}

export async function fetch_host_spec(): Promise<HostSpec> {
  return (await api("/api/host")).json();
}

export async function fetch_models(): Promise<ModelInfo[]> {
  const res = await fetch(`${API_BASE}/api/models`);
  if (!res.ok) throw new Error(`${res.status} fetching models`);
  return res.json();
}

export async function select_model(name: string): Promise<void> {
  const res = await fetch(`${API_BASE}/api/models/select`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name }),
  });
  if (!res.ok) throw new Error(await res.text());
}

export async function codex_start(): Promise<{ authorize_url: string }> {
  const res = await fetch(`${API_BASE}/api/auth/codex/start`, { method: "POST" });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export interface CodexStatus {
  status: "logged_in" | "expired" | "missing" | "awaiting_login";
  cli_available?: boolean;
  error?: string;
}

export async function codex_status(): Promise<CodexStatus> {
  const res = await fetch(`${API_BASE}/api/auth/codex/status`);
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function fetch_codex_models(): Promise<CodexModel[]> {
  const res = await fetch(`${API_BASE}/api/auth/codex/models`);
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function chat_codex(message: string, model: string): Promise<ChatReply> {
  const res = await fetch(`${API_BASE}/api/chat/codex`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ message, model }),
  });
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function claude_start(): Promise<{ authorize_url: string }> {
  const res = await fetch(`${API_BASE}/api/auth/claude/start`, { method: "POST" });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function claude_callback(code: string): Promise<void> {
  const res = await fetch(`${API_BASE}/api/auth/claude/callback`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code }),
  });
  if (!res.ok) throw new Error(await res.text());
}

export async function claude_status(): Promise<CodexStatus> {
  const res = await fetch(`${API_BASE}/api/auth/claude/status`);
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function chat_claude(message: string): Promise<ChatReply> {
  const res = await fetch(`${API_BASE}/api/chat/claude`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ message }),
  });
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export interface AlertSettings {
  webhook_set: boolean;
}

export async function fetch_alert_settings(): Promise<AlertSettings> {
  const res = await fetch(`${API_BASE}/api/settings/alerts`);
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function save_alert_settings(webhook_url: string | null): Promise<AlertSettings> {
  const res = await fetch(`${API_BASE}/api/settings/alerts`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ webhook_url }),
  });
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function fetch_zai_settings(): Promise<ZaiSettings> {
  const res = await fetch(`${API_BASE}/api/settings/zai`);
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function save_zai_schedule(
  say_hi_time: string | null,
  say_hi_interval_mins: number | null,
  timezone: string | null
): Promise<ZaiSettings> {
  const res = await fetch(`${API_BASE}/api/settings/zai`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ say_hi_time, say_hi_interval_mins, timezone }),
  });
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function save_zai_settings(api_key: string, model: string): Promise<ZaiSettings> {
  const res = await fetch(`${API_BASE}/api/settings/zai`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ api_key, model }),
  });
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export interface LocalModelSettings {
  endpoint: string;
}

export async function fetch_local_settings(): Promise<LocalModelSettings> {
  return (await api("/api/settings/local")).json();
}

export async function save_local_settings(endpoint: string): Promise<void> {
  await api("/api/settings/local", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ endpoint }),
  });
}

export async function zai_model_action(req: ZaiModelAction): Promise<ZaiSettings> {
  const res = await fetch(`${API_BASE}/api/settings/zai/models`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export interface ZaiQuota {
  tokens_used_pct: number;
  time_limit_reset_ms: number | null;
  time_limit_pct: number | null;
}

export interface PlatformQuota {
  platform: string;
  available: boolean;
  reason: string | null;
  tokens_used_pct: number | null;
  window_reset_ms: number | null;
  window_used_pct: number | null;
}

export async function fetch_quota_board(): Promise<PlatformQuota[]> {
  const res = await fetch(`${API_BASE}/api/quota`);
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return (await res.json()).platforms;
}

export async function fetch_zai_quota(): Promise<ZaiQuota> {
  const res = await fetch(`${API_BASE}/api/settings/zai/quota`);
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export interface CodexUsageRow {
  captured_at: string;
  plan_type: string | null;
  primary_used_percent: number | null;
  primary_resets_at: string | null;
  secondary_used_percent: number | null;
  secondary_resets_at: string | null;
}

export async function fetch_codex_usage_latest(): Promise<CodexUsageRow | null> {
  const res = await fetch(`${API_BASE}/api/codex/usage/latest`);
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function fetch_codex_usage_history(limit = 20): Promise<CodexUsageRow[]> {
  const res = await fetch(`${API_BASE}/api/codex/usage/history?limit=${limit}`);
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function zai_say_hi(): Promise<{ ok: boolean; reply: string }> {
  const res = await fetch(`${API_BASE}/api/settings/zai/hi`, { method: "POST" });
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function chat_zai(
  message: string,
  model: string,
  system?: PromptSection[]
): Promise<ChatReply> {
  const res = await fetch(`${API_BASE}/api/chat/zai`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ message, model: model || undefined, system }),
  });
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function render_prompt(sections: PromptSection[]): Promise<RenderedPrompt> {
  const res = await fetch(`${API_BASE}/api/prompts/render`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ sections }),
  });
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function fetch_system_prompt(): Promise<string> {
  const res = await fetch(`${API_BASE}/api/settings/system-prompt`);
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return (await res.json()).prompt;
}

export async function save_system_prompt(prompt: string): Promise<string> {
  const res = await fetch(`${API_BASE}/api/settings/system-prompt`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ prompt }),
  });
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return (await res.json()).prompt;
}

export async function fetch_sandboxes(): Promise<SandboxDir[]> {
  const res = await fetch(`${API_BASE}/api/sandbox`);
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function purge_sandbox(pid: number): Promise<{ removed: boolean }> {
  const res = await fetch(`${API_BASE}/api/sandbox/purge`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ pid }),
  });
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function sweep_sandboxes(): Promise<{ removed: number }> {
  const res = await fetch(`${API_BASE}/api/sandbox/sweep`, { method: "POST" });
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export interface SandboxLogEntry {
  role: string;
  content: string;
}

export async function fetch_sandbox_logs(path: string): Promise<SandboxLogEntry[]> {
  const res = await fetch(`${API_BASE}/api/sandbox/logs?path=${encodeURIComponent(path)}`);
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function run_machine_agent(
  hostname: string,
  agent: string,
  cmd: string,
): Promise<{ output: string }> {
  const res = await fetch(
    `${API_BASE}/api/machines/${encodeURIComponent(hostname)}/agents/${encodeURIComponent(agent)}/run`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ cmd }),
    },
  );
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function fetch_workspaces(): Promise<Workspace[]> {
  return (await api("/api/workspaces")).json();
}

export async function create_workspace(name: string): Promise<Response> {
  return api("/api/workspaces", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name }),
  });
}

export async function fetch_projects(workspace_id: number): Promise<Project[]> {
  return (await api(`/api/workspaces/${workspace_id}/projects`)).json();
}

export async function create_project(workspace_id: number, name: string): Promise<Response> {
  return api(`/api/workspaces/${workspace_id}/projects`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name }),
  });
}

export async function fetch_cards(project_id: number | null): Promise<Card[]> {
  const qs = project_id == null ? "" : `?project_id=${project_id}`;
  return (await api(`/api/kanban/cards${qs}`)).json();
}

export async function create_card(
  project_id: number,
  column_id: string,
  title: string,
  description: string,
  priority: string,
  labels?: string[],
  checklist?: ChecklistItem[],
  estimate?: number | null
): Promise<Response> {
  return api("/api/kanban/cards", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      project_id,
      column_id,
      title,
      description,
      priority,
      labels: labels ? JSON.stringify(labels) : null,
      checklist: checklist ? JSON.stringify(checklist) : null,
      estimate: estimate ?? null,
    }),
  });
}

export async function move_card(id: number, column_id: string, position: number): Promise<Response> {
  return api(`/api/kanban/cards/${id}/move`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ column_id, position }),
  });
}

export async function remove_card(id: number): Promise<Response> {
  return api(`/api/kanban/cards/${id}`, { method: "DELETE" });
}

export async function update_card(
  id: number,
  title: string,
  description: string,
  assignee: string | null,
  priority?: string,
  deadline?: string | null,
  labels?: string[],
  checklist?: ChecklistItem[],
  estimate?: number | null
): Promise<Card> {
  const res = await api(`/api/kanban/cards/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      title,
      description,
      assignee,
      priority,
      deadline,
      labels: labels ? JSON.stringify(labels) : null,
      checklist: checklist ? JSON.stringify(checklist) : null,
      estimate: estimate ?? null,
    }),
  });
  return res.json();
}

export async function fetch_comments(card_id: number): Promise<Comment[]> {
  return (await api(`/api/kanban/cards/${card_id}/comments`)).json();
}

export async function add_comment(card_id: number, body: string): Promise<Comment> {
  const res = await api(`/api/kanban/cards/${card_id}/comments`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ body }),
  });
  return res.json();
}

const CHAT_MAX_TOKENS = 1024;

/// Ask the agent engine a question (same backend as the Chat page).
export async function chat(message: string): Promise<ChatReply> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  const token = get_token();
  if (token) headers["Authorization"] = `Bearer ${token}`;
  const res = await fetch(`${API_BASE}/api/chat`, {
    method: "POST",
    headers,
    body: JSON.stringify({ message, max_tokens: CHAT_MAX_TOKENS, search: "auto" }),
  });
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export interface HostSpec {
  hostname: string;
  os: string;
  arch: string;
  cpu_model: string;
  cpu_cores: number;
  memory_bytes: number;
}

export interface AgentBrief {
  name: string;
  runs: number;
  last_cmd: string | null;
}

export interface MachineView {
  hostname: string;
  os: string;
  arch: string;
  role: string;
  ram_gib: number;
  ok: boolean;
  local: boolean;
  client_id: number | null;
  agents: AgentBrief[];
  sandboxes: SandboxDir[];
}

export async function fetch_machines(): Promise<MachineView[]> {
  return (await api("/api/machines")).json();
}

export async function kick_machine(hostname: string): Promise<{ hostname: string; kicked: boolean }> {
  const res = await api(`/api/machines/${encodeURIComponent(hostname)}/kick`, { method: "POST" });
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function fetch_pipelines(): Promise<Pipeline[]> {
  return (await api("/api/pipelines")).json();
}

export interface MachineAgent {
  agent: string;
  work_tree: string;
  runs: number;
  last_cmd: string | null;
}

export interface AgentLogs {
  agent: string;
  work_tree: string;
  runs: number;
  transcript: string[];
  last_result: string | null;
  machine: string;
}

export interface ActivityEntry { id: number; kind: string; message: string; created_at: string }

export async function fetch_activity(): Promise<ActivityEntry[]> {
  return (await api("/api/activity")).json();
}

export interface CardAgentState {
  name: string | null;
  state: { run?: CardRun } | null;
}

export async function fetch_card_agent(id: number): Promise<CardAgentState> {
  return (await api(`/api/kanban/cards/${id}/agent`)).json();
}

export async function fetch_machine_agents(): Promise<MachineAgent[]> {
  const data: { agents: MachineAgent[] } = await (await api("/api/manager/agents")).json();
  return data.agents;
}

export async function fetch_agent_logs(agent: string): Promise<AgentLogs> {
  return (await api(`/api/manager/agents/${encodeURIComponent(agent)}/logs`)).json();
}

export interface AgentWhere {
  agent: string;
  machine: string;
  local: boolean;
}

/** Which machine an agent runs on — works for local and remote agents. */
export async function fetch_agent_machine(agent: string): Promise<AgentWhere> {
  const res = await api(`/api/agents/whereis/${encodeURIComponent(agent)}`);
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res.json();
}

export async function run_agent_command(agent: string, cmd: string): Promise<string> {
  const res: { agent: string; output: string } = await (
    await api(`/api/manager/agents/${encodeURIComponent(agent)}/run`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ cmd }),
    })
  ).json();
  return res.output;
}

export interface AgentOutput {
  id: number;
  agent: string;
  result: string | null;
  patch: string;
  commit_oid: string | null;
  transcript: string;
  status: "pending" | "approved" | "rejected";
  created_at: string;
}

export async function fetch_agent_outputs(status?: string): Promise<AgentOutput[]> {
  const query = status ? `?status=${encodeURIComponent(status)}` : "";
  return (await api(`/api/agent-outputs${query}`)).json();
}

export async function set_agent_output_status(id: number, status: AgentOutput["status"]): Promise<void> {
  await api(`/api/agent-outputs/${id}/status`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ status }),
  });
}

export type PortKind = "any" | "text" | "json" | "image";

export interface StagePorts {
  input: PortKind;
  output: PortKind;
  wired: boolean;
  doc: string;
  params: { key: string; hint: string; required: boolean }[];
}

export type PipelineSchema = Record<string, StagePorts>;

export async function fetch_pipeline_schema(): Promise<PipelineSchema> {
  return (await api("/api/pipelines/schema")).json();
}

export interface PipelineRunStage {
  node: string;
  stage: string;
  status: "ok" | "failed";
  note: string;
}

export interface PipelineRunRecord {
  pipeline_id: number;
  pipeline_name: string;
  status: "ok" | "failed";
  stages: PipelineRunStage[];
  output?: string | null;
  resources: string[];
  finished_at: string;
}

export async function test_pipeline(
  id: number,
  input: string
): Promise<PipelineRunRecord> {
  const res = await api(`/api/pipelines/${id}/test`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ input }),
  });
  return res.json();
}

export async function fetch_card_resources(
  card_id: number
): Promise<{ id: number; name: string; content: string; created_at: string }[]> {
  return (await api(`/api/kanban/cards/${card_id}/resources`)).json();
}

export async function create_pipeline(name: string, spec: PipelineSpec): Promise<Pipeline> {
  const res = await api("/api/pipelines", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name, spec }),
  });
  return res.json();
}

export async function update_pipeline(
  id: number | "new",
  name: string,
  spec: PipelineSpec
): Promise<Response> {
  return api(`/api/pipelines/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name, spec }),
  });
}

export async function remove_pipeline(id: number): Promise<Response> {
  return api(`/api/pipelines/${id}`, { method: "DELETE" });
}

export interface UploadReply {
  path: string;
}

export async function upload_attachment(file: File): Promise<UploadReply> {
  const body = new FormData();
  body.append("file", file);
  const res = await api("/api/attachments", { method: "POST", body });
  return res.json();
}

export async function set_card_pipeline(id: number, pipeline_id: number | null): Promise<Response> {
  return api(`/api/kanban/cards/${id}/pipeline`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ pipeline_id }),
  });
}

export async function run_card(id: number): Promise<CardRun> {
  const res = await api(`/api/kanban/cards/${id}/run`, { method: "POST" });
  return res.json();
}

export async function set_card_schedule(id: number, cron: string | null): Promise<Response> {
  return api(`/api/kanban/cards/${id}/schedule`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ cron }),
  });
}

export async function fetch_cronjobs(): Promise<CronJob[]> {
  return (await api("/api/cronjobs")).json();
}

export async function fetch_agents(): Promise<Agent[]> {
  return (await api("/api/agents")).json();
}

export async function create_agent(fields: Omit<Agent, "id">): Promise<Agent> {
  const res = await api("/api/agents", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(fields),
  });
  return res.json();
}

export async function update_agent(
  id: number | "new",
  fields: Omit<Agent, "id">
): Promise<Response> {
  return api(`/api/agents/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(fields),
  });
}

export async function remove_agent(id: number | "new"): Promise<Response> {
  return api(`/api/agents/${id}`, { method: "DELETE" });
}

export async function set_agent(
  id: number,
  name: string,
  state: unknown
): Promise<Response> {
  return api(`/api/kanban/cards/${id}/agent`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name, state }),
  });
}
