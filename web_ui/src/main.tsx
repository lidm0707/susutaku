import React, { useEffect } from "react";
import { useState } from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter, Navigate, Route, Routes, useNavigate } from "react-router-dom";
import ChatModal from "./components/ChatModal.jsx";
import AgentSettings from "./features/agents/AgentSettings.jsx";
import Kanban from "./pages/Kanban.jsx";
import Login from "./pages/Login.jsx";
import Settings from "./pages/Settings.jsx";
import Sandbox from "./pages/Sandbox.jsx";
import Pipelines from "./pages/Pipelines.jsx";
import Cronjobs from "./pages/Cronjobs.jsx";
import { get_token } from "./lib.js";
import { Toaster } from "./ui/Toast.jsx";
import SideNav from "./components/SideNav.tsx";
import { WorkspaceProvider } from "./components/WorkspaceContext.tsx";
import { ProjectProvider } from "./components/ProjectContext.tsx";
import "./styles.css";

function RequireAuth({ children }: { children: React.ReactNode }) {
  if (!get_token()) return <Navigate to="/" replace />;
  return <>{children}</>;
}

function App() {
  const nav = useNavigate();
  const [chat_open, set_chat_open] = useState(false);
  useEffect(() => {
    const onUnauthorized = () => {
      if (window.location.pathname !== "/") nav("/", { replace: true });
    };
    window.addEventListener("susutaku:unauthorized", onUnauthorized);
    return () => window.removeEventListener("susutaku:unauthorized", onUnauthorized);
  }, [nav]);
  return (
    <WorkspaceProvider>
      <ProjectProvider>
        <div className="bg-art" aria-hidden="true" />
        <Routes>
        <Route path="/" element={<Login />} />
        <Route path="/login" element={<Navigate to="/" replace />} />
        <Route path="/chat" element={<Navigate to="/kanban" replace />} />
        <Route path="/models" element={<Navigate to="/kanban" replace />} />
        <Route path="/kanban" element={<RequireAuth><Kanban /></RequireAuth>} />
        <Route path="/pipelines" element={<RequireAuth><Pipelines /></RequireAuth>} />
        <Route path="/cronjobs" element={<RequireAuth><Cronjobs /></RequireAuth>} />
        <Route path="/agents" element={<RequireAuth><AgentSettings /></RequireAuth>} />
        <Route path="/settings" element={<RequireAuth><Settings /></RequireAuth>} />
        <Route path="/sandbox" element={<RequireAuth><Sandbox /></RequireAuth>} />
        <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
        <SideNav on_chat={() => set_chat_open(true)} />
        <ChatModal open={chat_open} on_close={() => set_chat_open(false)} />
        </ProjectProvider>
      <Toaster />
    </WorkspaceProvider>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <BrowserRouter>
      <App />
    </BrowserRouter>
  </React.StrictMode>
);
