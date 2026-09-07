import { useEffect, useState } from "react";
import { CheckCircle2, Info, XCircle } from "lucide-react";

const TIMEOUT_MS = 4000;

export type ToastKind = "error" | "success" | "info";

const KINDS: Record<ToastKind, typeof Info> = {
  error: XCircle,
  success: CheckCircle2,
  info: Info,
};

interface ToastItem {
  id: number;
  message: string;
  kind: ToastKind;
}

const listeners = new Set<(item: ToastItem) => void>();
let seq = 0;

export function toast(message: string, kind: ToastKind = "error"): void {
  const item: ToastItem = { id: ++seq, message, kind };
  listeners.forEach((fn) => fn(item));
}

export function Toaster() {
  const [items, setItems] = useState<ToastItem[]>([]);

  useEffect(() => {
    const push = (item: ToastItem) => {
      setItems((cur) => [...cur, item]);
      setTimeout(() => {
        setItems((cur) => cur.filter((t) => t.id !== item.id));
      }, TIMEOUT_MS);
    };
    listeners.add(push);
    return () => {
      listeners.delete(push);
    };
  }, []);

  if (items.length === 0) return null;
  return (
    <div className="toaster">
      {items.map((t) => {
        const Icon = KINDS[t.kind] || Info;
        return (
          <div key={t.id} className={`toast ${t.kind}`}>
            <Icon size={14} />
            <span>{t.message}</span>
          </div>
        );
      })}
    </div>
  );
}
