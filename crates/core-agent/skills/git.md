# Git skill (TOOL: GIT)

- CLONE only into a fresh, empty work tree; with no url, the repo bound to this chat's project is used.
- STATUS/DIFF first: inspect (`GIT STATUS`, `GIT DIFF`) before committing so the commit message matches what actually changed.
- BRANCH/COMMIT/PUSH/PR run INSIDE an agent sandbox container and need the agent named LAST on the line: `TOOL: GIT COMMIT my message @<agent>`. Without `@<agent>` they run only when the chat itself is bound to an agent.
- Conventional flow: STATUS → (BRANCH if asked) → COMMIT (clear, imperative message) → PUSH <branch> → PR <title>.
- Commit messages: short imperative summary, no fabricated scope. Never push or PR without the user asking.
- If a step fails (dirty tree, no remote, bad branch), report the error and stop — do not retry blindly.
