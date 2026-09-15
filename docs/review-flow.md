# Review flow: agent work tree → diff → commit → push → PR → artifact

How the review surfaces work end to end, what runs where, what is persisted,
and the failure modes that were hit and fixed (Sep 2026).

## The flow

```
card/chat run                review page (/review)                artifact
┌──────────────┐   spawn    ┌────────────────────────┐  finish   ┌──────────────────┐
│ agent works  │──────────▶│ status · diff (DiffView)│─────────▶│ agent_outputs     │
│ in its work  │           │ commit · push · open pr │           │ patch + commit +  │
│ tree         │           └────────────────────────┘           │ transcript        │
└──────────────┘                                                └──────────────────┘
```

1. **Spawn** — the first git op or manager call auto-spawns the agent's slot:
   work tree seeded at `work/agents/<agent>/sandbox/workspace` (clone of the
   project's bound repo, or an empty init), optional task branch created.
2. **Review** — `/review` shows status, the diff, and the commit/push/PR
   actions per agent.
3. **Finish** — `POST /api/manager/agents/{agent}/finish` captures
   everything into `TaskOutcome` and stores it as an agent output
   (`agent_outputs` table): patch text, commit oid, transcript. This is the
   durable artifact — reviewable in the outputs modal even after the work
   tree is gone.

## What runs where (git split)

| op | runs | notes |
|---|---|---|
| clone / status / diff | host, `git-rs` (git2) | read-only + bootstrap; no network needed after clone |
| branch / commit / push / pr | inside the agent's podman container (`NetworkPolicyChoice::Enabled`) | commit needs no token; push/pr get a run-scoped `GIT_TOKEN` via askpass |

## Diff resolution order (`manager-rs/src/git_state.rs`)

1. **Unborn HEAD** (fresh work tree, no commits): diff against the empty
   tree — normally empty, so `(no changes)`. Never a 400.
2. **Work-tree patch** (staged + unstaged + untracked vs HEAD): shown
   highlighted while the agent works.
3. **Clean tree → committed-but-unpushed fallback**
   (`GitRepo::patch_unpushed`): `merge-base(base, HEAD) → HEAD`, base being
   local `main` or `origin/main` (`git-rs` consts `DEFAULT_BRANCH`,
   `REMOTE_ORIGIN`). Lets review show committed work *after* a commit.
4. Nothing of the above → `(no changes)`, rendered by the highlighted
   `DiffView` (`web_ui/src/components/DiffView.tsx`): green adds, red dels,
   blue hunks, dim file metadata.

## Work tree persistence (deploy stack)

- `AGENTS_ROOT = "work/agents"` is **relative to the backend CWD** — in the
  deploy container that is `/app/work/agents`.
- `docker/compose/deploy.yml` mounts the named volume **`work:/app/work`**;
  without it every `up --build` (container recreate) silently destroyed all
  agent work trees and the review page 400'd with
  `failed to resolve path ... No such file or directory`.
- The volume only protects **future** work: anything committed must be
  pushed (or finished, to store the patch artifact) to be safe. Manager
  slots are in-memory and die with the container regardless.

## Failure modes hit and fixed

| symptom | cause | fix |
|---|---|---|
| review page silently does nothing on commit | `agent_git` (`web_ui/src/lib.ts`) ignored non-OK responses → `undefined` output → swallowed | throws `git <op> failed: <status> <body>`; the UI toast shows it |
| commit fails: `podman exited with exit status: 1 (missing podman or image ...?)` on an **empty** work tree | `git commit` exits 1 on "nothing to commit"; empty stderr triggered the generic message | commit script is `git add -A && (git commit ... \|\| git diff --quiet --cached)` — empty commit is a clean no-op (`git_in_sandbox.rs`, both `core-agent` and `manager-rs` copies) |
| diff 400 for brand-new agents | unborn HEAD crashed `head_oid()` | empty-tree diff + `has_commits` guard (see resolution order) |
| all work trees vanished after `up --build` | `work/agents` was in the container layer | `work` volume (above) |

Diagnostic tip: the generic podman message means **podman ran but exited
non-zero with empty stderr** — the real failure is usually on stdout
(e.g. git saying "nothing to commit"). Reproduce with the exact env:

```sh
docker exec susutaku-deploy-backend-1 podman run --rm \
  -v /app/work/agents/<agent>/sandbox/workspace:/workspace -w /workspace \
  -e HOME=/workspace -e USER=sandbox \
  -e PATH=/usr/bin:/bin:/usr/sbin:/sbin \
  localhost/susutaku-sandbox:latest /bin/bash -c 'set -e; cd "$HOME"; git status'
```

## E2E coverage

`playwright/tests/review.spec.ts` (runs against any live stack, no
mock-model dependency):

```sh
E2E_SKIP_DB_LIFECYCLE=1 npx playwright test tests/review.spec.ts
```

- fresh agent work tree: status "no commits", diff renders `(no changes)`
  in the DiffView (regression guard for the unborn-HEAD 400)
- commit via the UI: empty-tree commit succeeds as a no-op; then a file is
  created through a sandbox run, the UI commit lands, status flips to a
  real `HEAD <oid>` + `clean`, diff stays `(no changes)` (unpushed fallback
  finds no base branch on an e2e work tree)

## Card mentions vs review vs pipeline

- **Question on a card** → comment `@agent <question>`: routes to the
  agent's chat; tools (GIT STATUS/DIFF, SHELL, board ops) run per the
  agent's allow-list. Requires a **real model** — the deploy stack defaults
  to the mock model (`SUSUTAKU_LOCAL_MODEL_URL: http://mock-model:8992`),
  which only echoes canned `TOOLCALL-OK` replies. Point it at a real
  server, e.g. `SUSUTAKU_LOCAL_MODEL_URL=http://host.docker.internal:8992`.
- **Review / ship the work** → `/review` page (this doc).
- **Scheduled/whole-card automation** → attach a pipeline or a cron routine
  to the card; not the right tool for one-off questions.
