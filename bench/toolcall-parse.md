# Toolcall parse bench — 2026-09-18

Branch: `feature/agent-coding-flow` · Test: `backend/tests/toolcall_parse_bench.rs`
Run: `cargo test -p backend --test toolcall_parse_bench -- --nocapture`

## Results

| group    | before fix       | after fix              | speed          |
|----------|------------------|------------------------|----------------|
| stable   | 15/15            | **17/17** (asserted)   | ~1.2 us/parse  |
| knowngap | 1/3 parsed       | 0/1 (see below)        | ~1.4 us/parse  |

Parsing is cheap (microseconds); **stability, not speed, was the problem.**

## Fixes shipped (domain/service/tool_call.rs)

1. **Mid-prose markers** — `parse` now scans every word-boundary
   `TOOL:` occurrence (case-insensitive) instead of only line starts.
   `"I'll run it now. TOOL: SHELL ls -la"` parses.
2. **Truncated XML** — `param_value` is lenient about a missing closing
   `</parameter>`/`</invoke>` (cut-off stream/reply no longer drops the
   call). A partial `<invoke name="shell">…` now parses.

Both cases migrated from known-gap into the asserted stable group.

## Remaining known gap (parser cannot fix)

- **XML with a missing required parameter** (`coding` without `code`):
  there is no value to parse into. The right fix is a format-repair hint
  from the chat loop when `offers() == true` but `parse() == None`
  ("parameter `code` is required for coding") so the model retries with
  the missing part instead of a generic failure round.

## Verdict on the current approach

The dual text protocol (`TOOL:` line + Anthropic-style XML fallback) is
the right call for a local-model setup where native tool-call APIs are
not guaranteed — model-agnostic, stream-safe, cheap. With the two
parser fixes the failure surface shrinks to genuinely malformed calls;
halving to one canonical syntax in prompts is still worth doing.

## Next steps

- [ ] format-repair hint on `offers() && !parse()` in the chat loop
- [ ] one canonical tool syntax in prompts; the other stays fallback
- [ ] keep this bench as the acceptance gate (stable must stay 17/17)
