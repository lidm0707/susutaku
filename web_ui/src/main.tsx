import React, { useEffect } from "react";
import { useState } from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter, Navigate, Route, Routes, useNavigate } from "react-router-dom";
import ChatModal, { CHAT_PARAM, use_chat_docked } from "./components/ChatModal.jsx";
import AgentSettings from "./features/agents/AgentSettings.jsx";
import Task from "./pages/Task.jsx";
import Login from "./pages/Login.jsx";
import Settings from "./pages/Settings.jsx";
import Sandbox from "./pages/Sandbox.jsx";
import Attachments from "./pages/Attachments.jsx";
import Routines from "./pages/Routines.tsx";
import { get_token, set_query_param } from "./lib.js";
import { Toaster } from "./ui/Toast.jsx";
import SideNav from "./components/SideNav.tsx";
import { WorkspaceProvider } from "./components/WorkspaceContext.tsx";
import { ProjectProvider } from "./components/ProjectContext.tsx";
import "./styles.css";

function RequireAuth({ children }: { children: React.ReactNode }) {
  if (!get_token()) return <Navigate to={`/${window.location.search}`} replace />;
  return <>{children}</>;
}

function App() {
  const nav = useNavigate();
  const [chat_open, set_chat_open] = useState(
    () => new URLSearchParams(window.location.search).has(CHAT_PARAM),
  );
  const docked = use_chat_docked();
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
        <div className={chat_open && docked ? "chat-dock-main" : ""}>
        <Routes>
        <Route path="/" element={<Login />} />
        <Route path="/login" element={<Navigate to="/" replace />} />
        <Route path="/chat" element={<Navigate to={`/task${window.location.search}`} replace />} />
        <Route path="/models" element={<Navigate to="/task" replace />} />
        <Route path="/task" element={<RequireAuth><Task /></RequireAuth>} />
        <Route path="/routine" element={<Navigate to="/routines" replace />} />
        <Route path="/routines" element={<RequireAuth><Routines /></RequireAuth>} />
        <Route path="/attachments" element={<RequireAuth><Attachments /></RequireAuth>} />
        <Route path="/agents" element={<RequireAuth><AgentSettings /></RequireAuth>} />

        <Route path="/settings" element={<RequireAuth><Settings /></RequireAuth>} />
        <Route path="/sandbox" element={<RequireAuth><Sandbox /></RequireAuth>} />
        <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
        </div>
        <SideNav on_chat={() => set_chat_open((o) => !o)} chat_open={chat_open} shifted={chat_open && docked} />
        <ChatModal open={chat_open} on_close={() => { set_chat_open(false); set_query_param(CHAT_PARAM, null); }} />
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
