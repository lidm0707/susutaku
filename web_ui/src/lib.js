export const API_BASE = import.meta.env.VITE_API_BASE || "";

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
