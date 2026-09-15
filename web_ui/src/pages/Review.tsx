import { useEffect, useState } from "react";
import { FileDiff, GitCommitHorizontal, GitPullRequestArrow, RefreshCw, Upload, User } from "lucide-react";
import { Button, Field, TextInput } from "../ui/controls.js";
import { Modal } from "../ui/Overlay.js";
import { use_projects } from "../components/ProjectContext.js";
import { agent_git, fetch_agents, fetch_manager_agents, type AgentGitBody, type ManagerAgent } from "../lib.js";
import DiffView, { DiffFileList, DiffStats, type DiffFile } from "../components/DiffView.js";

const TASK_COMMIT_PREFIX = "agent task: ";
const NO_CHANGES = "(no changes)";

// merge flow steps, shown so the commit → push → pr order is obvious
const FLOW_STEPS = [
  "1. commit — save the changes inside the agent's work tree",
  "2. push — upload the branch to the remote",
  "3. open pr — request merging that branch into main",
] as const;

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
  const { project_id } = use_projects();
  const projectId = project_id ?? undefined;

  const agent = picked ?? agents[0]?.agent ?? null;
  const info = agents.find((a) => a.agent === agent);

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

  const clean = status.includes("clean");
  const dirty = Boolean(status) && !clean;

  const action_value = action === "commit" ? message : action === "push" ? branch : title;
  const set_action_value = action === "commit" ? setMessage : action === "push" ? setBranch : setTitle;

  return (
    <main className="chat review-page">
      <header>
        <h1>review</h1>
        <span className="sub">{agents.length} agent{agents.length === 1 ? "" : "s"}</span>
      </header>
      <div className="review-grid">
        <section className="review-left" aria-label="changes">
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
              work tree: <code title={info.work_tree}>{info.work_tree}</code>
            </p>
          )}
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
              {diff && diff !== NO_CHANGES && (
                <button type="button" className="review-diff-open" onClick={() => setFullDiff(true)}>
                  view full diff
                </button>
              )}
            </div>
            {diff && diff !== NO_CHANGES ? (
              <DiffFileList diff={diff} on_pick={setFile} />
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
        </section>
        <section className="review-right" aria-label="actions">
          <ol className="review-flow">
            {FLOW_STEPS.map((s) => <li key={s}>{s}</li>)}
          </ol>
          <button type="button" className="review-action" disabled={busy || !agent} onClick={() => setAction("commit")}>
            <GitCommitHorizontal size={14} /> commit…
          </button>
          <button type="button" className="review-action" disabled={busy || !agent} onClick={() => setAction("push")}>
            <Upload size={14} /> push…
          </button>
          <button type="button" className="review-action" disabled={busy || !agent} onClick={() => setAction("pr")}>
            <GitPullRequestArrow size={14} /> open pr…
          </button>
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
        open={fullDiff}
        title={status.includes("clean") || !status ? "diff" : "git status + diff"}
        on_close={() => setFullDiff(false)}
        wide
      >
        {status && (
          <Field label="git status">
            <pre className="agent-review-output">{status}</pre>
          </Field>
        )}
        <Field label="diff">
          <DiffView diff={diff} rows={24} />
        </Field>
      </Modal>
    </main>
  );
}
