import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter, Route, Routes } from "react-router-dom";
import Chat from "./pages/Chat.jsx";
import Models from "./pages/Models.jsx";
import Settings from "./pages/Settings.jsx";
import Sandbox from "./pages/Sandbox.jsx";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root")).render(
  <React.StrictMode>
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Chat />} />
        <Route path="/models" element={<Models />} />
        <Route path="/settings" element={<Settings />} />
        <Route path="/sandbox" element={<Sandbox />} />
      </Routes>
    </BrowserRouter>
  </React.StrictMode>
);
