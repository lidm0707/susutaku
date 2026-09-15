import { useEffect, useState } from "react";
import { GitCommitHorizontal, GitPullRequestArrow, RefreshCw, Upload } from "lucide-react";
import { SlideOver } from "../ui/Overlay.js";
import { Button, Field, TextArea, TextInput } from "../ui/controls.js";
import { agent_git, type AgentGitBody } from "../lib.js";

const EMPTY_DIFF = "(no changes)";

export default function AgentReview({
  open,
  agent,
  project_id,
  on_close,
}: {
  open: boolean;
  agent: string | null;
  project_id?: number;
  on_close: () => void;
}) {
  const [status, setStatus] = useState("");
  const [diff, setDiff] = useState("");
  const [message, setMessage] = useState("");
  const [branch, setBranch] = useState("");
  const [title, setTitle] = useState("");
  const [output, setOutput] = useState("");
  const [busy, setBusy] = useState(false);

  async function run(body: AgentGitBody) {
    if (!agent) return;
    setBusy(true);
    try {
      return await agent_git(agent, body, project_id);
    } finally {
      setBusy(false);
    }
  }

  async function refresh() {
    if (!agent) return;
    const [st, d] = await Promise.all([
      agent_git(agent, { op: "status" }, project_id),
      agent_git(agent, { op: "diff" }, project_id),
    ]);
    setStatus(st.trim());
    setDiff(d.trim() || EMPTY_DIFF);
  }

  useEffect(() => {
    if (!open || !agent) return;
    setStatus("");
    setDiff("");
    setOutput("");
    refresh().catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, agent]);

  async function act(body: AgentGitBody, done: string) {
    const out = await run(body);
    setOutput(`$ ${body.op}\n${(out ?? "").trim() || "(ok)"}`);
    await refresh().catch(() => {});
  }

  const clean = status.includes("clean");

  return (
    <SlideOver open={open && agent !== null} title={`review · ${agent ?? ""}`} on_close={on_close}>
      <section className="agent-review">
        <p className="agent-review-status">{status || "…"}</p>
        <Field label="diff" icon={<RefreshCw size={12} />}>
          <TextArea readOnly value={diff} rows={12} spellCheck={false} />
        </Field>
        <Field label="commit message" icon={<GitCommitHorizontal size={12} />}>
          <TextInput
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            placeholder="what changed…"
          />
        </Field>
        <Button
          variant="primary"
          disabled={busy || !message.trim()}
          onClick={() => act({ op: "commit", message: message.trim() }, "committed").then(() => setMessage(""))}
        >
          <GitCommitHorizontal size={14} /> commit
        </Button>
        <Field label="branch to push" icon={<Upload size={12} />}>
          <TextInput
            value={branch}
            onChange={(e) => setBranch(e.target.value)}
            placeholder="task/… or feature/…"
          />
        </Field>
        <Button
          variant="primary"
          disabled={busy || !branch.trim()}
          onClick={() => act({ op: "push", branch: branch.trim() }, "pushed")}
        >
          <Upload size={14} /> push
        </Button>
        <Field label="pr title" icon={<GitPullRequestArrow size={12} />}>
          <TextInput
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="pull request title…"
          />
        </Field>
        <Button
          variant="primary"
          disabled={busy || !title.trim()}
          onClick={() => act({ op: "pr", title: title.trim() }, "pr opened")}
        >
          <GitPullRequestArrow size={14} /> open pr
        </Button>
        {output && <pre className="agent-review-output">{output}</pre>}
        {!clean && status && <p className="agent-review-hint">work tree dirty — commit before push</p>}
      </section>
    </SlideOver>
  );
}
