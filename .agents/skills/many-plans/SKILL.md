---
name: many-plans
description: Decompose long multi-issue user prompts into numbered .plans files (00-99), implement them one by one with verification, and keep context compact across sessions. Use when the user pastes a long numbered list of issues/feature requests or says "make plans for all this".
---

# Many-plans workflow

Handle a long prompt containing many issues without losing track or blowing context.

## 1. Decompose first, code later

- Read the whole prompt. Split it into atomic, independently-verifiable issues.
- Group issues that touch the same files ONLY if they must ship together; otherwise one plan per issue.
- Write one plan file per issue into `./.plans` using the next free two-digit-derived number
  (find the highest existing number in `.plans`, start from the next one).
- Plan file format: `NN-short-slug.md` with sections `## Problem`, `## Fix` (file paths!),
  `## Acceptance`. Keep each under ~40 lines — it is a contract, not an essay.

## 2. Order the work

- Sort plans: trivial frontend fixes first, cross-cutting features next, backend/infra last.
- Issues that only need documentation/clarification get a plan too, but are flagged `docs-only`.

## 3. Execute one plan at a time

- For each plan, in order:
  1. Implement exactly what `## Fix` says; nothing extra.
  2. Verify the `## Acceptance` criteria (build, test, or render check — `design_render` bin
     for UI pages, `cargo check && cargo clippy` for Rust, `make e2e` for UI flows).
  3. Tick the plan off by appending `## Status: done <date>` to the plan file.
  4. If a discovery invalidates the plan, rewrite the plan BEFORE continuing to code.
- Parallelize only across disjoint file sets (sub-agents with explicit write scopes).

## 4. Compact context between plans

- After finishing each plan, do NOT carry its full diff in your head. The plan file + git diff
  are the memory. If the session gets long, write a short handoff note at the bottom of the
  current plan: files touched, follow-ups, open questions.
- Never re-read large files you already edited; use grep with line context instead.

## 5. Report

- Final message: one line per plan — number, what changed, how it was verified.
- Anything not finished stays as its plan file with `## Status: open` and the exact blocker.
