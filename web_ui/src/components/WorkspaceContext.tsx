import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { fetch_workspaces, get_token, type Workspace } from "../lib.js";

interface WorkspaceCtxValue {
  workspaces: Workspace[];
  ws_id: number | null;
  pick: (id: number) => void;
  reload: () => Promise<void>;
}

const WorkspaceCtx = createContext<WorkspaceCtxValue | null>(null);

export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const [workspaces, set_workspaces] = useState<Workspace[]>([]);
  const [ws_id, set_ws_id] = useState<number | null>(null);

  async function reload() {
    if (!get_token()) return;
    const list = await fetch_workspaces();
    set_workspaces(list);
    set_ws_id((cur) => (cur != null && list.some((w) => w.id === cur) ? cur : list[0]?.id ?? null));
  }

  useEffect(() => {
    reload().catch(() => {});
  }, []);

  return (
    <WorkspaceCtx.Provider value={{ workspaces, ws_id, pick: set_ws_id, reload }}>
      {children}
    </WorkspaceCtx.Provider>
  );
}

export function use_workspaces(): WorkspaceCtxValue {
  const ctx = useContext(WorkspaceCtx);
  if (!ctx) throw new Error("use_workspaces requires WorkspaceProvider");
  return ctx;
}
