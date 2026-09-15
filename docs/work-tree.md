# The agent work tree: what it is and how it works

A beginner-friendly tour of the single most important directory in the
agent system. Read this before `docs/review-flow.md` (the shipping flow)
or `docs/podman-sandbox.md` (the jail around agent commands).

## 1. The git concept (background)

In git, a **work tree** (working tree) is the directory where a checkout
actually lives — the files you see and edit — as opposed to the `.git`
database next to it, which stores the history. One repo, one work tree,
many branches; checking out a branch rewrites the work tree files.

```
my-repo/            ← this whole folder is "the work tree"
├── .git/           ← history, branches, staging area (never edited by hand)
├── src/…           ← the actual files ("working tree")
└── README.md
```

Every agent gets exactly one such folder to work in. That is what
"susutaku work tree" means below.

## 2. One work tree per agent

The manager gives every agent its own folder under one root
(`AGENTS_ROOT = "work/agents"`, `crates/manager-rs/src/manager.rs` — a
relative path, resolved against the backend process CWD):

```
work/agents/
├── zai/                        ← everything for agent "zai" (task-less slot)
│   └── sandbox/
│       └── workspace/          ← THE work tree: a git repo checkout
│           ├── .git/
│           └── <project files>
├── zai_42/                     ← a TASK slot: agent "zai" on task 42
│   └── sandbox/workspace/      ← its own repo clone + task branch
└── …
```

One work tree per agent — and, since the per-task-slot rework, one extra
work tree **per (agent, task) pair**: `spawn_task` keys the slot as
`<agent>#<task>` (`TASK_KEY_SEP`, sanitized to `_` on disk), so two tasks of
the same agent never share a tree or a branch.

Why one per agent, not one shared folder:

- **isolation** — two agents can never overwrite each other's files, and
  one agent's uncommitted mess never shows up in another's diff
- **clean diffs** — review always answers "what did *this* agent change"
  (see `docs/review-flow.md`, diff resolution order)
- **safe teardown** — finishing an agent deletes its folder; nobody else
  can be hurt by that

Inside, the path has a fixed shape (`core-agent/src/podman/{sandbox,state}.rs`):

| path | role |
|---|---|
| `work/agents/<name>/` | the agent's slot dir (manager's unit of ownership) |
| `…/sandbox/` | sandbox metadata + transcript state (json) |
| `…/sandbox/workspace/` | the git work tree; mounted at `/workspace` inside the container |

## 3. Lifecycle

### spawn (first git op or manager call)

1. slot key computed: `<agent>` or `<agent>#<task>`; path computed:
   `work/agents/<sanitized key>` (`spawn_task_impl`)
2. `reclaim_stale`: if the folder exists it is wiped (slots are in-memory; a
   tree on disk with no live slot is stale by definition)
3. seed: clone the project's bound repo (`settings → git repos`) — or, if
   none is bound / the clone fails, `git init` an empty repo
4. task branch (task slots only, best effort, **only when the repo already
   has commits**): `task/<task>-<agent>` (`TASK_BRANCH_PREFIX`) is created
   and checked out, so the task's commits stay isolated from `main`; the
   spawn-time HEAD is remembered as the slot's `base_commit`
5. the podman sandbox is pointed at this folder; every agent command runs
   with `workspace/` mounted at `/workspace` — the agent can only see and
   change files inside its own work tree

### during runs

Git ops are split (details in `docs/git-sandbox-tls.md`):

- **status / diff** run host-side against `workspace/` — fast, read-only
- **branch / commit / push / pr** run *inside* the container, where the
  agent's environment lives; push/pr get the repo token injected per run

### finish / teardown

`manager.finish` (via `finish_with_push`) captures the **task patch** with
`GitRepo::task_patch`: any still-uncommitted work is committed as
`agent task: <agent>` (`TASK_COMMIT_PREFIX`), then the patch is the diff from
the spawn-time `base_commit` (or the empty tree on an unborn HEAD) to the new
HEAD. The outcome (`TaskOutcome`) carries patch, commit oid, the task branch
and — when the finish request asked for a push — the push result. The push
(`FinishPush`, repo + token from the project's bound repo, branch = override
else task branch else current) runs **before** teardown so a commit can never
be stranded on a deleted tree; a push failure does not fail the finish, the
patch artifact is already captured. Everything is stored in the
`agent_outputs` table (the durable artifact), then the sandbox is purged, the
whole `work/agents/<slot>` folder deleted and the slot dropped. After finish,
the only memory of the work tree is that artifact.

Publishing for review is the same shape one step earlier:
`Manager::publish` commits pending work, pushes the task branch and opens a
PR against `main` (GitHub API) — it requires a task-scoped slot, since the
branch to publish is the one created at spawn.

## 4. Where the root lives (host vs docker)

| run style | CWD of backend | work trees live at | survives redeploys? |
|---|---|---|---|
| host (`make backend`) | repo root | `<repo>/work/agents/` | yes (plain files; it's gitignored) |
| deploy compose | `/app` in container | `/app/work/agents/` → named volume **`work`** | yes, since the `work:/app/work` volume — before it existed, every `up --build` wiped all work trees |
| playwright/e2e stacks | in container | same as deploy | volume or ephemeral per stack |

The deploy volume is declared in `docker/compose/deploy.yml`
(`work:/app/work` + top-level `work:`). If you ever see
`failed to resolve path 'work/agents/…': No such file or directory`, the
folder is gone but a stale in-memory slot still points at it — restart the
backend (slots are in-memory only) and the next git op re-seeds.

## 5. Look inside (useful commands)

```sh
# on the deploy stack
docker exec susutaku-deploy-backend-1 ls /app/work/agents
docker exec susutaku-deploy-backend-1 \
  git -C /app/work/agents/zai/sandbox/workspace status
docker exec susutaku-deploy-backend-1 \
  git -C /app/work/agents/zai/sandbox/workspace log --oneline -5

# on host runs — it is just a normal repo:
git -C work/agents/zai/sandbox/workspace diff
```

Anything you can do with `git` locally, you can do here — the work tree is
a plain repository, the agent just works in it inside a jail.

## 6. Rules worth remembering

1. **Push (or finish) before redeploying.** The folder survives redeploys
   now, but manager slots do not — and only pushed commits / the stored
   patch artifact are truly safe.
2. **Uncommitted work is fragile.** It exists only in `workspace/`.
3. **`git status` on the review page is your window** into this folder —
   `HEAD <oid>` + `clean`/`dirty` is literally the state of
   `…/sandbox/workspace`.
4. **The agent never leaves its work tree**: the container bind-mount is
   the only writable path it has, and the sandbox refuses to run
   unsandboxed.

## 7. Related docs

- `docs/work-tree.md` — the agent work tree itself (lifecycle, layout, persistence)
- `docs/review-flow.md` — diff/commit/push/PR flow built on the work tree
- `docs/git-sandbox-tls.md` — why git ops are split host vs container
- `docs/podman-sandbox.md` — the jail around agent commands
