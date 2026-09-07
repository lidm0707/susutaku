import { NavLink, useNavigate } from "react-router-dom";
import {
  Bot,
  Clock,
  KanbanSquare,
  LogOut,
  MessageSquareText,
  Settings,
  SlidersHorizontal,
  Workflow,
} from "lucide-react";
import { clear_token, get_token } from "../lib.js";

const ITEMS = [
  { to: "/chat", title: "chat", Icon: MessageSquareText },
  { to: "/kanban", title: "kanban", Icon: KanbanSquare },
  { to: "/pipelines", title: "pipelines", Icon: Workflow },
  { to: "/cronjobs", title: "cronjobs", Icon: Clock },
  { to: "/prompts", title: "prompts", Icon: SlidersHorizontal },
  { to: "/agents", title: "agents", Icon: Bot },
  { to: "/settings", title: "settings", Icon: Settings },
];

export default function SideNav() {
  const nav = useNavigate();
  if (!get_token()) return null;
  function logout() {
    clear_token();
    nav("/", { replace: true });
  }
  return (
    <nav className="dock-nav" aria-label="main navigation">
      {ITEMS.map(({ to, title, Icon }) => (
        <NavLink key={to} to={to} title={title} className={({ isActive }) => (isActive ? "active" : "")}>
          <Icon size={16} />
        </NavLink>
      ))}
      <button className="dock-logout" onClick={logout} title="logout / switch user">
        <LogOut size={16} />
      </button>
    </nav>
  );
}
