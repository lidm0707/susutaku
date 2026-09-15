# Git toolcalls: "there is no TLS stream available" and how agent git works

For the user-facing review flow built on these ops (diff fallbacks, artifact
capture, review page e2e) see `docs/review-flow.md`.

## Symptom

The agent's `GIT CLONE` failed instantly with:

```
git: there is no TLS stream available
```

`/workspace` stayed empty; `GIT STATUS` showed nothing; retrying produced the
same error every time.

## Root cause

`crates/git-rs/Cargo.toml` declared:

```toml
git2 = { version = "0.20", default-features = false }
```

`default-features = false` strips git2's **`https`** feature, so libgit2 is
built with **no TLS backend at all**. Any `https://` remote fails *before any
network I/O* with exactly `there is no TLS stream available`. The error is a
compile-time capability gap, not a network/DNS/proxy problem — retrying,
switching networks, or "trying later" can never fix it.

Key discriminator for next time:

| error | meaning |
|---|---|
| `there is no TLS stream available` | libgit2 built without TLS → dependency/features bug |
| `failed to resolve address` / timeout | network, DNS, or firewall problem |
| `401` / `authentication failed` | missing or bad token |

## Fix

`crates/git-rs/Cargo.toml`:

```toml
git2 = { version = "0.20", default-features = false, features = [
    "https",
    "vendored-openssl",
] }
```

- `https` — registers libgit2's TLS stream (OpenSSL backend).
- `vendored-openssl` — builds OpenSSL from source so it compiles cleanly on
  macOS (no brew openssl version dance) and in the Debian backend container.

Restart the backend after rebuilding so the new binary is picked up.

Verified by `crates/git-rs/tests/clone_tls.rs`: a real
`GitRepo::clone_into("https://github.com/octocat/Hello-World.git")` over TLS.

## How agent git is wired (context)

Split of responsibilities — host = network + credentials, container = files
+ execution:

| op | runs where | why |
|---|---|---|
| `GIT CLONE` | host (git-rs/git2) into the agent's workspace | bootstrap; token lives in the project repo binding |
| `GIT STATUS` / `GIT DIFF` | host (git-rs/git2) | read-only inspection |
| `GIT BRANCH` / `GIT COMMIT` / `GIT PUSH` / `GIT PR` | inside the agent's own podman container (`NetworkPolicyChoice::Enabled`) | the agent does its git work in its own environment; host never commits or pushes for it |

In-container auth uses a container-local GIT_ASKPASS reading `$GIT_TOKEN`
(injected per run via `Sandbox::run_with_env`), so the token never lands in
the workspace, `.git/config`, argv, or the transcript. PRs go through the
GitHub API with curl; `$GIT_TOKEN` is expanded by the shell at runtime — the
header must stay in **double quotes** in the generated script or curl sends
the literal string `$GIT_TOKEN`.

See `.plans/106-agent-in-sandbox-git-work.md` and
`crates/core-agent/src/podman/git_in_sandbox.rs`.

## Checklist when git clone fails again

1. Read the exact error; classify with the table above.
2. If TLS-stream: check `git-rs` features (`cargo tree -p git-rs -i openssl-sys`
   should show libgit2-sys linked against openssl, not missing).
3. If network-ish: test on the host first (`git ls-remote <url>`), remember
   the *sandbox* has `--network=none` by design — only the in-sandbox git ops
   get outbound network.
4. If auth: bind the repo + token in settings → git repos; tokens never come
   from the model.
