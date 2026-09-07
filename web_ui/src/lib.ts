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

export interface ZaiSettings {
  api_key_set: boolean;
  model: string;
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

export async function fetch_bootstrap(): Promise<{ username: string; role: string }> {
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

export async function fetch_zai_settings(): Promise<ZaiSettings> {
  const res = await fetch(`${API_BASE}/api/settings/zai`);
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

export async function fetch_cards(project_id: number): Promise<Card[]> {
  return (await api(`/api/kanban/cards?project_id=${project_id}`)).json();
}

export async function create_card(
  project_id: number,
  column_id: string,
  title: string,
  description: string,
  priority: string
): Promise<Response> {
  return api("/api/kanban/cards", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ project_id, column_id, title, description, priority }),
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
  assignee: string | null
): Promise<Card> {
  const res = await api(`/api/kanban/cards/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ title, description, assignee }),
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

export async function fetch_pipelines(): Promise<Pipeline[]> {
  return (await api("/api/pipelines")).json();
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

export async function set_card_pipeline(id: number, pipeline_id: number | null): Promise<Response> {
  return api(`/api/kanban/cards/${id}/pipeline`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ pipeline_id }),
  });
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
