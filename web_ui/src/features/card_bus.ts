import { useEffect, useRef } from "react";

export interface CardEvent {
  id: number;
  title: string;
  project_id: number | null;
}

const listeners = new Set<(card: CardEvent) => void>();

export function emit_card_created(card: CardEvent) {
  listeners.forEach((fn) => fn(card));
}

export function use_card_created(fn: (card: CardEvent) => void) {
  const ref = useRef(fn);
  ref.current = fn;
  useEffect(() => {
    const handler = (card: CardEvent) => ref.current(card);
    listeners.add(handler);
    return () => {
      listeners.delete(handler);
    };
  }, []);
}
