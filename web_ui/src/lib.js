export const API_BASE = import.meta.env.VITE_API_BASE || "";

const TOKEN_KEY = "susutaku_token";

export function get_token() {
  return localStorage.getItem(TOKEN_KEY) || "";
}

export function save_token(token) {
  localStorage.setItem(TOKEN_KEY, token);
}

export function clear_token() {
  localStorage.removeItem(TOKEN_KEY);
}

async function api(path, opts = {}) {
  const token = get_token();
  const headers = { ...(opts.headers || {}) };
  if (token) headers.Authorization = `Bearer ${token}`;
  const res = await fetch(`${API_BASE}${path}`, { ...opts, headers });
  if (!res.ok) {
    const err = new Error(`${res.status} ${await res.text()}`);
    err.status = res.status;
    throw err;
  }
  return res;
}

export async function login(username, password) {
  const res = await api("/api/auth/login", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ username, password }),
  });
  const data = await res.json();
  save_token(data.token);
  return data;
}

export async function logout() {
  try {
    await api("/api/auth/logout", { method: "POST" });
  } finally {
    clear_token();
  }
}

export async function fetch_bootstrap() {
  return (await api("/api/auth/bootstrap")).json();
}

export async function change_password(old_password, new_password) {
  return api("/api/auth/change-password", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ old_password, new_password }),
  });
}

export async function register_user(username, password, role) {
  const res = await api("/api/auth/users", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ username, password, role }),
  });
  return res.json();
}

export async function fetch_users() {
  return (await api("/api/auth/users")).json();
}

const GIB = 1024 ** 3;

export function pretty_name(name) {
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

export function size_label(bytes) {
  return `${(bytes / GIB).toFixed(1)} GB`;
}

export async function fetch_models() {
  const res = await fetch(`${API_BASE}/api/models`);
  if (!res.ok) throw new Error(`${res.status} fetching models`);
  return res.json();
}

export async function select_model(name) {
  const res = await fetch(`${API_BASE}/api/models/select`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name }),
  });
  if (!res.ok) throw new Error(await res.text());
}

export async function codex_start() {
  const res = await fetch(`${API_BASE}/api/auth/codex/start`, { method: "POST" });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function codex_status() {
  const res = await fetch(`${API_BASE}/api/auth/codex/status`);
  if (!res.ok) throw new Error(`${res.status} fetching codex status`);
  return res.json();
}

export async function fetch_codex_models() {
  const res = await fetch(`${API_BASE}/api/auth/codex/models`);
  if (!res.ok) throw new Error(`${res.status} fetching codex models`);
  return res.json();
}

export async function chat_codex(message, model) {
  const res = await fetch(`${API_BASE}/api/chat/codex`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ message, model }),
  });
  if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
  return res.json();
}

export async function fetch_zai_settings() {
  const res = await fetch(`${API_BASE}/api/settings/zai`);
  if (!res.ok) throw new Error(`${res.status} fetching zai settings`);
  return res.json();
}

export async function save_zai_settings(api_key, model) {
  const res = await fetch(`${API_BASE}/api/settings/zai`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ api_key, model }),
  });
  if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
  return res.json();
}

export async function chat_zai(message, model) {
  const res = await fetch(`${API_BASE}/api/chat/zai`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ message, model: model || undefined }),
  });
  if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
  return res.json();
}

export async function fetch_sandboxes() {
  const res = await fetch(`${API_BASE}/api/sandbox`);
  if (!res.ok) throw new Error(`${res.status} fetching sandboxes`);
  return res.json();
}

export async function purge_sandbox(pid) {
  const res = await fetch(`${API_BASE}/api/sandbox/purge`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ pid }),
  });
  if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
  return res.json();
}

export async function sweep_sandboxes() {
  const res = await fetch(`${API_BASE}/api/sandbox/sweep`, { method: "POST" });
  if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
  return res.json();
}

export async function fetch_workspaces() {
  return (await api("/api/workspaces")).json();
}

export async function create_workspace(name) {
  return api("/api/workspaces", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name }),
  });
}

export async function fetch_projects(workspace_id) {
  return (await api(`/api/workspaces/${workspace_id}/projects`)).json();
}

export async function create_project(workspace_id, name) {
  return api(`/api/workspaces/${workspace_id}/projects`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name }),
  });
}

export async function fetch_cards(project_id) {
  return (await api(`/api/kanban/cards?project_id=${project_id}`)).json();
}

export async function create_card(project_id, column_id, title, description, priority) {
  return api("/api/kanban/cards", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ project_id, column_id, title, description, priority }),
  });
}

export async function move_card(id, column_id, position) {
  return api(`/api/kanban/cards/${id}/move`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ column_id, position }),
  });
}

export async function remove_card(id) {
  return api(`/api/kanban/cards/${id}`, { method: "DELETE" });
}

export async function fetch_pipelines() {
  return (await api("/api/pipelines")).json();
}

export async function create_pipeline(name, spec) {
  const res = await api("/api/pipelines", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name, spec }),
  });
  return res.json();
}

export async function update_pipeline(id, name, spec) {
  return api(`/api/pipelines/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name, spec }),
  });
}

export async function remove_pipeline(id) {
  return api(`/api/pipelines/${id}`, { method: "DELETE" });
}

export async function set_card_pipeline(id, pipeline_id) {
  return api(`/api/kanban/cards/${id}/pipeline`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ pipeline_id }),
  });
}

export async function fetch_agents() {
  return (await api("/api/agents")).json();
}

export async function create_agent(fields) {
  const res = await api("/api/agents", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(fields),
  });
  return res.json();
}

export async function update_agent(id, fields) {
  return api(`/api/agents/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(fields),
  });
}

export async function remove_agent(id) {
  return api(`/api/agents/${id}`, { method: "DELETE" });
}

export async function set_agent(id, name, state) {
  return api(`/api/kanban/cards/${id}/agent`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name, state }),
  });
}
