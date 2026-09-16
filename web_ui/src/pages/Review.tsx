import { useEffect, useRef, useState } from "react";
import { FileDiff, GitCommitHorizontal, GitPullRequestArrow, RefreshCw, Upload, User } from "lucide-react";
import { Button, Field, TextInput } from "../ui/controls.js";
import { Modal } from "../ui/Overlay.js";
import { use_projects } from "../components/ProjectContext.js";
import { agent_git, fetch_agents, fetch_card_runs, fetch_cards, fetch_manager_agents, finish_manager_agent, type AgentGitBody, type Card, type CardRunRecord, type ManagerAgent } from "../lib.js";
import DiffView, { DiffFileList, DiffStats, type DiffFile } from "../components/DiffView.js";

const TASK_COMMIT_PREFIX = "agent task: ";
const NO_CHANGES = "(no changes)";
const WORK_TREE_ROOT = "work/agents";
const LEFT_MIN_PCT = 15;
const LEFT_MAX_PCT = 60;
const LEFT_DEFAULT_PCT = 30;


type ActionKind = "commit" | "push" | "pr";

const ACTION_LABEL: Record<ActionKind, string> = {
  commit: "commit",
  push: "push",
  pr: "open pr",
};

const ACTION_HINT: Record<ActionKind, string> = {
  commit: "saves all changes in the agent's work tree as one commit",
  push: "uploads the branch to the remote — commit first",
  pr: "opens a pull request to merge this branch into main",
};

function prefill(name: string) {
  return {
    message: `${TASK_COMMIT_PREFIX}${name}`,
    branch: `task/${name}`,
    title: `${TASK_COMMIT_PREFIX}${name}`,
  };
}

export default function Review() {
  const [agents, setAgents] = useState<ManagerAgent[]>([]);
  const [picked, setPicked] = useState<string | null>(null);
  const [status, setStatus] = useState("");
  const [diff, setDiff] = useState("");
  const [output, setOutput] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [branch, setBranch] = useState("");
  const [title, setTitle] = useState("");
  const [action, setAction] = useState<ActionKind | null>(null);
  const [file, setFile] = useState<DiffFile | null>(null);
  const [fullDiff, setFullDiff] = useState(false);
  const [tasks, setTasks] = useState<Card[]>([]);
  const [task_runs, set_task_runs] = useState<{ card: Card; runs: CardRunRecord[] } | null>(null);
  const [runs_busy, set_runs_busy] = useState(false);
  const grid_ref = useRef<HTMLDivElement>(null);
  const [left_pct, set_left_pct] = useState(LEFT_DEFAULT_PCT);
  const { project_id } = use_projects();
  const projectId = project_id ?? undefined;

  const agent = picked ?? agents[0]?.agent ?? null;
  const info = agents.find((a) => a.agent === agent);
  const agent_tasks = tasks.filter((t) => t.agent_name === agent);

  useEffect(() => {
    fetch_cards(projectId ?? null)
      .then(setTasks)
      .catch(() => {});
  }, [projectId]);

  useEffect(() => {
    // manager snapshot first; fall back to configured agents so review works
    // for an agent that was never spawned (git ops auto-spawn its slot)
    Promise.all([fetch_manager_agents().catch(() => []), fetch_agents().catch(() => [])])
      .then(([rows, configured]) => {
        const known = new Map<string, ManagerAgent>();
        for (const a of configured) {
          known.set(a.name, { agent: a.name, work_tree: "", runs: 0, last_cmd: null });
        }
        for (const r of rows) known.set(r.agent, r);
        const merged = [...known.values()];
        setAgents(merged);
        const first = merged[0];
        if (first) {
          const p = prefill(first.agent);
          setMessage(p.message);
          setBranch(p.branch);
          setTitle(p.title);
        }
      })
      .catch(() => {});
  }, []);

  async function refresh() {
    if (!agent) return;
    setBusy(true);
    try {
      const [st, d] = await Promise.all([
        agent_git(agent, { op: "status" }, projectId),
        agent_git(agent, { op: "diff" }, projectId),
      ]);
      setStatus(st.trim());
      setDiff(d.trim() || NO_CHANGES);
      // the first op auto-spawns the slot — pick up its work tree
      fetch_manager_agents()
        .then((rows) => setAgents((cur) => {
          const known = new Map(cur.map((a) => [a.agent, a]));
          for (const r of rows) known.set(r.agent, r);
          return [...known.values()];
        }))
        .catch(() => {});
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    setStatus("");
    setDiff("");
    setOutput("");
    refresh().catch(() => {});
    if (agent) {
      const p = prefill(agent);
      setMessage(p.message);
      setBranch(p.branch);
      setTitle(p.title);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agent]);

  async function act(body: Parameters<typeof agent_git>[1]) {
    if (!agent) return;
    setBusy(true);
    try {
      const out = await agent_git(agent, body, projectId);
      setOutput(`$ ${body.op}\n${out.trim() || "(ok)"}`);
      // The PR is the end of a work tree's life: the branch is on the remote,
      // so tear the tree down instead of keeping it listed here. Refreshing
      // git status would auto-spawn a fresh slot, so just drop the agent.
      if (body.op === "pr") {
        try {
          await finish_manager_agent(agent);
          setAgents((cur) => cur.filter((a) => a.agent !== agent));
          setStatus("");
          setDiff("");
          return;
        } catch {
          // tree stays alive (e.g. agent on another machine) — keep reviewing
        }
      }
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  async function run_action() {
    if (!action) return;
    const body: AgentGitBody =
      action === "commit" ? { op: "commit", message } :
      action === "push" ? { op: "push", branch: branch.trim() } :
      { op: "pr", title: title.trim() };
    setAction(null);
    await act(body);
  }

  async function open_task(t: Card) {
    set_task_runs({ card: t, runs: [] });
    set_runs_busy(true);
    try {
      const runs = await fetch_card_runs(t.id);
      set_task_runs({ card: t, runs });
    } catch {
      set_task_runs({ card: t, runs: [] });
    } finally {
      set_runs_busy(false);
    }
  }

  const clean = status.includes("clean");
  const dirty = Boolean(status) && !clean;

  const action_value = action === "commit" ? message : action === "push" ? branch : title;
  const set_action_value = action === "commit" ? setMessage : action === "push" ? setBranch : setTitle;

  function start_resize(e: React.MouseEvent) {
    e.preventDefault();
    const grid = grid_ref.current;
    if (!grid) return;
    const on_move = (ev: MouseEvent) => {
      const rect = grid.getBoundingClientRect();
      const pct = ((ev.clientX - rect.left) / rect.width) * 100;
      set_left_pct(Math.min(LEFT_MAX_PCT, Math.max(LEFT_MIN_PCT, pct)));
    };
    const on_up = () => {
      window.removeEventListener("mousemove", on_move);
      window.removeEventListener("mouseup", on_up);
    };
    window.addEventListener("mousemove", on_move);
    window.addEventListener("mouseup", on_up);
  }

  return (
    <main className="chat review-page">
      <header>
        <h1>review</h1>
        <span className="sub">{agents.length} agent{agents.length === 1 ? "" : "s"}</span>
      </header>
      <div
        className="review-grid"
        ref={grid_ref}
        style={{ "--review-left": `${left_pct}%` } as React.CSSProperties}
      >
        <section className="review-left" aria-label="changes">
          <div className="review-actions" role="toolbar" aria-label="git actions">
            <button type="button" className="review-action-circle" disabled={busy || !agent} onClick={() => setAction("commit")}>
              <GitCommitHorizontal size={16} />
              <span className="review-action-tip">commit…</span>
            </button>
            <button type="button" className="review-action-circle" disabled={busy || !agent} onClick={() => setAction("push")}>
              <Upload size={16} />
              <span className="review-action-tip">push…</span>
            </button>
            <button type="button" className="review-action-circle" disabled={busy || !agent} onClick={() => setAction("pr")}>
              <GitPullRequestArrow size={16} />
              <span className="review-action-tip">open pr…</span>
            </button>
          </div>
          <nav className="review-agent-list">
            {agents.length === 0 && <p className="empty">no agents — create one on the agents page</p>}
            {agents.map((a) => (
              <button
                key={a.agent}
                className={a.agent === agent ? "agent-item active" : "agent-item"}
                onClick={() => setPicked(a.agent)}
              >
                <User size={14} />
                <span className="agent-item-name">{a.agent}</span>
                <span className="agent-item-model">{a.runs} runs</span>
              </button>
            ))}
          </nav>
          {info && (
            <p className="review-worktree">
              work tree: <code title={info.work_tree || `${WORK_TREE_ROOT}/${info.agent}`}>
                {info.work_tree || `${WORK_TREE_ROOT}/${info.agent}`}
              </code>
            </p>
          )}
          <section className="review-tasks" aria-label="tasks for this agent">
            <h2 className="review-tasks-title">tasks</h2>
            {agent_tasks.length === 0 ? (
              <p className="review-tasks-empty">no tasks assigned to this agent</p>
            ) : (
              <ul className="review-task-list">
                {agent_tasks.map((t) => (
                  <li key={t.id} className="review-task-item">
                    <button
                      type="button"
                      className="review-task-open"
                      title="show run history"
                      onClick={() => open_task(t).catch(() => {})}
                    >
                      <span className="review-task-title">{t.title}</span>
                    </button>
                    <span className="review-task-column">{t.column_id}</span>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </section>
        <div
          className="review-resizer"
          role="separator"
          aria-orientation="vertical"
          aria-label="resize columns"
          onMouseDown={start_resize}
        />
        <section className="review-right" aria-label="changes and tasks">
          <div className="review-diff-panel">
            <div className="review-diff-head">
              <span className="review-diff-title"><FileDiff size={14} /> changes</span>
              {diff && diff !== NO_CHANGES
                ? <DiffStats diff={diff} />
                : <span className="diff-stats">{dirty ? "untracked changes only" : "no changes"}</span>}
              <span className={`review-badge ${clean ? "ok" : dirty ? "dirty" : ""}`}>
                {clean ? "clean" : dirty ? "dirty" : "…"}
              </span>
              <button
                type="button"
                className="review-diff-open"
                onClick={() => refresh().catch(() => {})}
                disabled={busy}
                title="re-read status and diff from the work tree"
              >
                <RefreshCw size={12} /> refresh
              </button>
            </div>
            {diff && diff !== NO_CHANGES ? (
              <>
                <DiffFileList diff={diff} on_pick={setFile} />
                <DiffView diff={diff} rows={28} />
              </>
            ) : (
              <p className="review-diff-none">
                {busy ? "loading…" : !status
                  ? "…"
                  : info && info.runs === 0
                    ? "this agent has 0 runs — nothing was ever written to its work tree. work shown in chat threads does not appear here; run the agent first."
                    : clean
                      ? "nothing to review — work tree is clean"
                      : "no tracked changes"}
              </p>
            )}
            {dirty && (
              <button type="button" className="review-status-link" onClick={() => setFullDiff(true)}>
                show git status
              </button>
            )}
          </div>
          {output && <pre className="agent-review-output">{output}</pre>}
        </section>
      </div>

      <Modal open={action !== null} title={action ? ACTION_LABEL[action] : ""} on_close={() => setAction(null)}>
        <form
          className="modal-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (action_value.trim()) run_action();
          }}
        >
          {action && <p className="review-action-hint">{ACTION_HINT[action]}</p>}
          <Field label={action === "commit" ? "commit message" : action === "push" ? "branch" : "pr title"}>
            <TextInput
              autoFocus
              value={action_value}
              onChange={(e) => set_action_value(e.target.value)}
            />
          </Field>
          <Button variant="primary" type="submit" disabled={busy || !action_value.trim()}>
            {action ? ACTION_LABEL[action] : ""}
          </Button>
        </form>
      </Modal>

      <Modal open={file !== null} title={file?.path ?? ""} on_close={() => setFile(null)} wide>
        <DiffView diff={file?.text ?? ""} rows={24} />
      </Modal>

      <Modal
        open={task_runs !== null}
        title={task_runs ? `runs — ${task_runs.card.title}` : ""}
        on_close={() => set_task_runs(null)}
        wide
      >
        {task_runs && (
          <div className="review-runs">
            <p className="review-action-hint">
              status {task_runs.card.column_id}
              {task_runs.card.run_status ? ` · last run ${task_runs.card.run_status}` : ""}
            </p>
            {runs_busy ? (
              <p className="review-action-hint">loading…</p>
            ) : task_runs.runs.length === 0 ? (
              <p className="review-action-hint">no runs recorded — use “run now” on the card</p>
            ) : (
              <ul className="review-run-list">
                {task_runs.runs.map((r) => (
                  <li key={r.id} className="review-run-item">
                    <div className="review-run-head">
                      <span className={`review-badge ${r.ok ? "ok" : "dirty"}`}>
                        {r.ok ? "ok" : "failed"}
                      </span>
                      <span className="review-run-meta">
                        {r.trigger} · {r.agent} · {r.finished_at ?? r.started_at}
                      </span>
                    </div>
                    {r.summary && <pre className="review-run-summary">{r.summary}</pre>}
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </Modal>

      <Modal
        open={fullDiff}
        title="git status"
        on_close={() => setFullDiff(false)}
        wide
      >
        {status
          ? <pre className="agent-review-output">{status}</pre>
          : <p className="review-action-hint">no status yet — hit refresh</p>}
      </Modal>
    </main>
  );
}
