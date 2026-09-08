import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { fetch_projects, get_token, type Project } from "../lib.js";
import { use_workspaces } from "./WorkspaceContext.tsx";

interface ProjectCtxValue {
  projects: Project[];
  project_id: number | null;
  pick_project: (id: number | null) => void;
  reload_projects: () => Promise<void>;
}

const ProjectCtx = createContext<ProjectCtxValue | null>(null);

export function ProjectProvider({ children }: { children: ReactNode }) {
  const { ws_id } = use_workspaces();
  const [projects, set_projects] = useState<Project[]>([]);
  const [project_id, set_project_id] = useState<number | null>(null);

  async function reload_projects() {
    if (ws_id == null || !get_token()) {
      set_projects([]);
      set_project_id(null);
      return;
    }
    const list = await fetch_projects(ws_id);
    set_projects(list);
    set_project_id((cur) => (cur != null && list.some((p) => p.id === cur) ? cur : list[0]?.id ?? null));
  }

  useEffect(() => {
    reload_projects().catch(() => {
      set_projects([]);
      set_project_id(null);
    });
  }, [ws_id]);

  return (
    <ProjectCtx.Provider value={{ projects, project_id, pick_project: set_project_id, reload_projects }}>
      {children}
    </ProjectCtx.Provider>
  );
}

export function use_projects(): ProjectCtxValue {
  const ctx = useContext(ProjectCtx);
  if (!ctx) throw new Error("use_projects requires ProjectProvider");
  return ctx;
}
