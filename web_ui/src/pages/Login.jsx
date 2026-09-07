import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { ArrowLeft, KeyRound, LogIn } from "lucide-react";
import { change_password, clear_token, get_token, login } from "../lib.js";

export default function Login() {
  const nav = useNavigate();
  const [mode, setMode] = useState("login"); // "login" | "change"
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (get_token()) {
      clear_token();
    }
  }, []);

  async function submit(e) {
    e.preventDefault();
    setError("");
    setBusy(true);
    if (mode === "change") {
      try {
        await change_password(password, newPassword);
        nav("/");
      } catch (err) {
        setError(String(err.message || err));
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
      nav("/");
      return;
    } catch (err) {
      setError(String(err.message || err));
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
        <nav className="nav">
          <Link to="/" title="back to chat"><ArrowLeft size={16} /></Link>
        </nav>
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
