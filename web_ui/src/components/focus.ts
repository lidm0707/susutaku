import { useSyncExternalStore } from "react";

const FOCUS_MAX_CHARS = 1200;

export type Focus =
  | { kind: "card"; card: CardFocus }
  | { kind: "pipeline"; id: number; name: string };

export interface CardFocus {
  id: number;
  project_id: number | null;
  title: string;
  description: string;
  column_id: string;
  priority: string;
  assignee?: string | null;
  agent_name?: string;
}

let focus: Focus | null = null;
const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

export function set_focus(next: Focus | null) {
  focus = next;
  emit();
}

export function get_focus(): Focus | null {
  return focus;
}

export function use_focus(): Focus | null {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    get_focus
  );
}

function clamp(text: string): string {
  return text.length > FOCUS_MAX_CHARS ? `${text.slice(0, FOCUS_MAX_CHARS)}…` : text;
}

export function focus_label(focus: Focus | null): string {
  if (!focus) return "";
  if (focus.kind === "card") return `card #${focus.card.id}: ${focus.card.title}`;
  return `pipeline: ${focus.name}`;
}

export function focus_section(focus: Focus | null): string {
  if (!focus) return "";
  if (focus.kind === "pipeline") {
    return `[focus: the user is viewing pipeline "${focus.name}"]`;
  }
  const c = focus.card;
  const lines = [
    `[focus: the user is viewing kanban card #${c.id}]`,
    `title: ${c.title}`,
    `column: ${c.column_id} · priority: ${c.priority}`,
    c.assignee ? `assignee: ${c.assignee}` : "",
    c.agent_name ? `agent: ${c.agent_name}` : "",
    `description:`,
    c.description || "(empty)",
  ].filter(Boolean);
  return clamp(lines.join("\n"));
}
