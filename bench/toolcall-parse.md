# Toolcall parse bench — 2026-09-18

Branch: `feature/agent-coding-flow` · Test: `backend/tests/toolcall_parse_bench.rs`
Run: `cargo test -p backend --test toolcall_parse_bench -- --nocapture`

## Results (main, strict line scanner)

| group    | score            | speed            |
|----------|------------------|------------------|
| stable   | 15/15 correct    | ~1200 ns/parse   |
| knowngap | 1/3 parsed       | ~900 ns/parse    |

Parsing is cheap (µs); **stability, not speed, is the problem.**

## What works today

- clean `TOOL: KIND args` lines (any case), also fenced in markdown,
  after `</think>`, or after prose on an earlier line
- `GIT`, `LSP`, `GEOMATH` aliases and argument forms
- well-formed `<invoke name="...">` XML blocks (incl. `write_file` alias)

## Measured gaps (real replies that lose the tool call)

1. **mid-prose marker** — `I'll run it now. TOOL: SHELL ls -la` →
   the line scanner only accepts the marker at line start.
2. **partial XML** — an unterminated `<invoke>` block → `parse_xml` None,
   no `TOOL:` line to fall back on → dead end.
3. **XML with a missing parameter** → whole call dropped instead of an
   error fed back to the model.

`ToolCall::offers()` catches some of these to feed an error back, but the
run still burns a round without executing anything.

## Verdict on the current approach

The dual text protocol (`TOOL:` line + Anthropic-style XML fallback) is the
right call for a local-model setup where native tool-call APIs are not
guaranteed — it is model-agnostic, stream-safe, and cheap. But:

- two syntaxes means two failure surfaces; aliases (`write_file`,
  `find_card`, `geomath`) hint at models guessing the format. Every
  mismatch costs a full inference round.
- the strict line-start scan is the single biggest loss source (gap 1).
- error-feedback is binary (parsed / not); malformed calls should return a
  *repair hint* to the model ("marker must start the line") instead of a
  generic retry.

## Recommended next steps (in order)

1. scan for `TOOL:` mid-line at word boundaries (host working tree already
   has a candidate-scan prototype — `tool_arg_candidates` — port it);
2. on `offers() == true` but `parse() == None`, reply with a
   format-repair hint naming the expected syntax;
3. adopt one canonical syntax in the prompt and treat the other as
   fallback only (halves the failure surface);
4. keep this bench as the acceptance gate: stable 15/15 must hold, gap
   cases migrate into `stable` as the parser improves (target 3/3).
