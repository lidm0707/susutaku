import React, { useEffect } from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter, Navigate, Route, Routes, useNavigate } from "react-router-dom";
import Chat from "./pages/Chat.jsx";
import AgentSettings from "./pages/AgentSettings.jsx";
import Kanban from "./pages/Kanban.jsx";
import Login from "./pages/Login.jsx";
import Models from "./pages/Models.jsx";
import Settings from "./pages/Settings.jsx";
import Sandbox from "./pages/Sandbox.jsx";
import Pipelines from "./pages/Pipelines.jsx";
import { get_token } from "./lib.js";
import { Toaster } from "./ui/Toast.jsx";
import "./styles.css";

function RequireAuth({ children }: { children: React.ReactNode }) {
  if (!get_token()) return <Navigate to="/" replace />;
  return <>{children}</>;
}

function App() {
  const nav = useNavigate();
  useEffect(() => {
    const onUnauthorized = () => nav("/", { replace: true });
    window.addEventListener("susutaku:unauthorized", onUnauthorized);
    return () => window.removeEventListener("susutaku:unauthorized", onUnauthorized);
  }, [nav]);
  return (
    <>
      <Routes>
        <Route path="/" element={<Login />} />
        <Route path="/login" element={<Navigate to="/" replace />} />
        <Route path="/chat" element={<RequireAuth><Chat /></RequireAuth>} />
        <Route path="/models" element={<RequireAuth><Models /></RequireAuth>} />
        <Route path="/kanban" element={<RequireAuth><Kanban /></RequireAuth>} />
        <Route path="/pipelines" element={<RequireAuth><Pipelines /></RequireAuth>} />
        <Route path="/agents" element={<RequireAuth><AgentSettings /></RequireAuth>} />
        <Route path="/settings" element={<RequireAuth><Settings /></RequireAuth>} />
        <Route path="/sandbox" element={<RequireAuth><Sandbox /></RequireAuth>} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
      <Toaster />
    </>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <BrowserRouter>
      <App />
    </BrowserRouter>
  </React.StrictMode>
);
