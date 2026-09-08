import { useEffect, useState } from "react";
import { Navigate, useNavigate } from "react-router-dom";
import { KeyRound, LogIn, UserPlus } from "lucide-react";
import { change_password, fetch_bootstrap, get_token, login, register_user } from "../lib.js";
import { toast } from "../ui/Toast.jsx";

type Mode = "login" | "change" | "setup";

const MIN_PASSWORD_LEN = 8;

export default function Login() {
  const nav = useNavigate();
  const [mode, setMode] = useState<Mode>("login");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (get_token()) return;
    fetch_bootstrap()
      .then((b) => {
        if (b.needs_setup) setMode("setup");
      })
      .catch(() => undefined);
  }, []);

  if (get_token()) {
    return <Navigate to="/chat" replace />;
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    setBusy(true);
    if (mode === "setup") {
      try {
        await register_user(username.trim(), newPassword, "owner");
        const data = await login(username.trim(), newPassword);
        if (data.must_change_password) {
          setMode("change");
          setError("this account must set a new password first");
          return;
        }
        nav("/chat");
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        setError(msg);
        toast(msg, "error");
      } finally {
        setBusy(false);
      }
      return;
    }
    if (mode === "change") {
      try {
        await change_password(password, newPassword);
        nav("/chat");
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        setError(msg);
        toast(msg, "error");
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
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      toast(msg, "error");
    } finally {
      setBusy(false);
    }
  }

  const headline =
    mode === "change" ? "change password" : mode === "setup" ? "create owner account" : "login";
  const subline =
    mode === "change"
      ? "set a new password (min 8 chars) to continue"
      : mode === "setup"
        ? "first run — create the owner account (min 8 chars)"
        : "susutaku";

  return (
    <main className="chat login-page">
      <header>
        <h1>{headline}</h1>
        <span className="sub">{subline}</span>
      </header>
      {error && <p className="error">{error}</p>}
      <form className="login-box" onSubmit={submit}>
        {mode !== "change" && (
          <input
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            placeholder="username"
            autoFocus
          />
        )}
        {mode === "change" ? (
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="current password"
          />
        ) : null}
        {mode !== "change" && (
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder={mode === "setup" ? `password (min ${MIN_PASSWORD_LEN} chars)` : "password"}
          />
        )}
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
          disabled={
            busy ||
            (mode !== "change" && (!username.trim() || !password)) ||
            (mode === "change" && (!password || newPassword.length < MIN_PASSWORD_LEN)) ||
            (mode === "setup" && newPassword.length < MIN_PASSWORD_LEN)
          }
        >
          {mode === "change" ? (
            <><KeyRound size={14} /> set new password</>
          ) : mode === "setup" ? (
            <><UserPlus size={14} /> create owner</>
          ) : (
            <><LogIn size={14} /> login</>
          )}
        </button>
        {mode === "login" && (
          <p className="hint">forgot username or password? ask an admin to reset it</p>
        )}
      </form>
    </main>
  );
}
