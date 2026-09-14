# Coding skill (<invoke name="coding">)

- Writes a complete file into the project work tree. Use for creating or overwriting whole files — never for one-line edits.
- Send the COMPLETE file content; the tool overwrites, it does not patch. Partial content destroys the user's code.
- Path is workspace-relative (`src/main.rs`), no leading `/`, no `..`.
- Before writing, make sure the change fits the project: read the surrounding files first (SHELL `cat`, LSP) so names, style and module wiring match.
- After writing, verify when possible: `TOOL: SHELL cargo check` in the same flow before claiming success.
- This is the only tool that uses the XML invoke form; all other tools use plain `TOOL:` lines.
