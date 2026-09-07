import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter, Route, Routes } from "react-router-dom";
import Chat from "./pages/Chat.jsx";
import AgentSettings from "./pages/AgentSettings.jsx";
import Kanban from "./pages/Kanban.jsx";
import Login from "./pages/Login.jsx";
import Models from "./pages/Models.jsx";
import Settings from "./pages/Settings.jsx";
import Sandbox from "./pages/Sandbox.jsx";
import Pipelines from "./pages/Pipelines.jsx";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root")).render(
  <React.StrictMode>
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Chat />} />
        <Route path="/models" element={<Models />} />
        <Route path="/kanban" element={<Kanban />} />
        <Route path="/pipelines" element={<Pipelines />} />
        <Route path="/agents" element={<AgentSettings />} />
        <Route path="/login" element={<Login />} />
        <Route path="/settings" element={<Settings />} />
        <Route path="/sandbox" element={<Sandbox />} />
      </Routes>
    </BrowserRouter>
  </React.StrictMode>
);
