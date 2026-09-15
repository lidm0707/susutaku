import { useEffect, useState } from "react";
import { GitCommitHorizontal, GitPullRequestArrow, Upload, User } from "lucide-react";
import { Button, Field, TextArea, TextInput } from "../ui/controls.js";
import { use_projects } from "../components/ProjectContext.js";
import { agent_git, fetch_agents, fetch_manager_agents, type ManagerAgent } from "../lib.js";

const TASK_COMMIT_PREFIX = "agent task: ";

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
      setDiff(d.trim() || "(no changes)");
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

  const clean = status.includes("clean");

  return (
    <main className="chat review-page">
      <header>
        <h1>review</h1>
        <span className="sub">{agents.length} agent{agents.length === 1 ? "" : "s"}</span>
      </header>
      <div className="review-grid">
        <section className="review-left" aria-label="work tree">
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
          <Field label="status" icon={<GitCommitHorizontal size={12} />}>
            <TextArea readOnly value={status || "…"} rows={2} spellCheck={false} />
          </Field>
          <Field label="diff" icon={<GitCommitHorizontal size={12} />}>
            <TextArea readOnly value={diff} rows={16} spellCheck={false} />
          </Field>
        </section>
        <section className="review-right" aria-label="actions">
          <Field label="commit message" icon={<GitCommitHorizontal size={12} />}>
            <TextInput value={message} onChange={(e) => setMessage(e.target.value)} />
          </Field>
          <Button variant="primary" disabled={busy || !agent} onClick={() => act({ op: "commit", message })}>
            <GitCommitHorizontal size={14} /> commit
          </Button>
          <Field label="branch to push" icon={<Upload size={12} />}>
            <TextInput value={branch} onChange={(e) => setBranch(e.target.value)} />
          </Field>
          <Button variant="primary" disabled={busy || !branch.trim()} onClick={() => act({ op: "push", branch: branch.trim() })}>
            <Upload size={14} /> push
          </Button>
          <Field label="pr title" icon={<GitPullRequestArrow size={12} />}>
            <TextInput value={title} onChange={(e) => setTitle(e.target.value)} />
          </Field>
          <Button variant="primary" disabled={busy || !title.trim()} onClick={() => act({ op: "pr", title: title.trim() })}>
            <GitPullRequestArrow size={14} /> open pr
          </Button>
          {output && <pre className="agent-review-output">{output}</pre>}
          {!clean && status && <p className="agent-review-hint">work tree dirty — commit before push</p>}
        </section>
      </div>
    </main>
  );
}
