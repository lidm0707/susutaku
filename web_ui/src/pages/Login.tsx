import { useState } from "react";
import { Navigate, useNavigate } from "react-router-dom";
import { KeyRound, LogIn } from "lucide-react";
import { change_password, get_token, login } from "../lib.js";

type Mode = "login" | "change";

export default function Login() {
  const nav = useNavigate();
  const [mode, setMode] = useState<Mode>("login");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  if (get_token()) {
    return <Navigate to="/chat" replace />;
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    setBusy(true);
    if (mode === "change") {
      try {
        await change_password(password, newPassword);
        nav("/chat");
      } catch (err: unknown) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setBusy(false);
      }
      return;
    }
    try {
      const data = await login(username.trim(), password);
      if (data.must_change_password) {
        setMode("change");
        setError("this account must set a new password first");
        return;
      }
      nav("/chat");
      return;
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="chat login-page">
      <header>
        <h1>{mode === "change" ? "change password" : "login"}</h1>
        <span className="sub">
          {mode === "change" ? "set a new password (min 8 chars) to continue" : "susutaku"}
        </span>
      </header>
      {error && <p className="error" style={{ textAlign: "center" }}>{error}</p>}
      <form className="login-box" onSubmit={submit}>
        <input
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          placeholder="username"
          autoFocus
        />
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder="password"
        />
        {mode === "change" && (
          <input
            type="password"
            value={newPassword}
            onChange={(e) => setNewPassword(e.target.value)}
            placeholder="new password (min 8 chars)"
          />
        )}
        <button
          type="submit"
          disabled={busy || !username.trim() || !password || (mode === "change" && newPassword.length < 8)}
        >
          {mode === "change"
            ? <><KeyRound size={14} /> set new password</>
            : <><LogIn size={14} /> login</>}
        </button>
      </form>
    </main>
  );
}
