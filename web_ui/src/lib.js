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
