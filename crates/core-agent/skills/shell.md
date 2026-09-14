# Shell skill (TOOL: SHELL)

- Runs one shell command inside the agent sandbox (jailed filesystem, loopback-only network). One command per turn; chain with `&&` when needed.
- Prefer read-only inspection first: `ls`, `cat`, `grep -rn`, `find`, `head`, `wc` before anything mutating.
- The sandbox filesystem is not the user's machine — files you create live in the sandbox work tree.
- Commands have a timeout; avoid long-running or interactive commands (no editors, no watches).
- Quote paths with spaces. Check output for errors before building on it; never assume a command succeeded.
