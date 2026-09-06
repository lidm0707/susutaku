import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter, Route, Routes } from "react-router-dom";
import Chat from "./pages/Chat.jsx";
import Models from "./pages/Models.jsx";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root")).render(
  <React.StrictMode>
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Chat />} />
        <Route path="/models" element={<Models />} />
      </Routes>
    </BrowserRouter>
  </React.StrictMode>
);
