# LSP skill (TOOL: LSP)

- Exact code navigation via rust-analyzer over the project work tree: DEFINITION, REFERENCES, HOVER.
- Arguments: `<path> <line> <col>` — path is workspace-relative without spaces; line and col are 0-BASED (first line is 0, first column is 0).
- Position the cursor ON the symbol itself (start or middle of its identifier), not on whitespace or punctuation — otherwise the result is empty.
- Prefer LSP over SHELL grep for "where is this defined / used" and "what type is this": it is precise, not textual.
- Typical flow: DEFINITION to jump to source, REFERENCES to see every call site before refactoring, HOVER for types and docs.
- Works on the current work tree state — run it after edits reflect reality, before claiming a refactor is complete.
