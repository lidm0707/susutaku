# Attachments

`attachments/` is the storage directory for managing file attachments
(documents, images, parsed inputs) used by the backend and pipeline crates.

## Conventions

- One subdirectory per source item; keep original file + derived artifacts together.
- Do not commit binary attachments to git; runtime-generated content stays local.
- Referenced by `backend/` and `crates/` via relative path `attachments/` —
  resolve paths through configuration, never hardcode absolute paths.
