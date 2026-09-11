# bench: lsp-rs codec framing (2026-09-12)

## What
`lsp_rs::codec` read/write of Content-Length framed JSON-RPC messages.

## Guess (before measuring)
Framing cost is dominated by `BufReader::read_line` + one `read_exact` into a
reused buffer — should be ~µs per message, dwarfed by any real language
server (rust-analyzer: 10–100 ms per request). No optimization warranted.

## Summary
- Not benchmarked with a harness yet; single-message round-trip is covered by
  `crates/lsp-rs/tests/codec.rs`.
- Design already avoids per-message allocation on the read side (buffer is
  reused via `&mut Vec<u8>`); write side does one `format!` header + one
  `write_all`.
- Expectation: client overhead < 0.1% of rust-analyzer response latency.

## Follow-up
If LSP calls end up in agent hot paths, add a `cargo bench` (divan or criterion)
measuring msg/s for 1 KiB–1 MiB payloads.
