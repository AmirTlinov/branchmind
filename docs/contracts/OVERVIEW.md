# Contracts — Overview (v3 MCP surface)

This folder defines the active MCP contract for BranchMind.

## Version

- Active contract version: **v3**.
- Advertised tools: **`think`**, **`branch`**, **`merge`**.
- Input style: markdown-only ` ```bm ... ``` ` command blocks.
- Legacy tool names are fail-closed with `UNKNOWN_TOOL`.

## Active contract index

- `V3_MCP_SURFACE.md` — tools, verbs, strict parser rules.
- `TYPES.md` — response envelope + typed error model.
- `MEMORY.md` — branch/commit/merge data model.
- `INTEGRATION.md` — deterministic cross-tool invariants.
- `PARITY.md` — v3 parity target and non-goals.

## Archived docs (de-indexed)

The following are not active contracts and must not be used as current behavior:

- `V1_*.md`
- `*_v1.md`
- legacy portal/ops migration docs

## Related

- `../GLOSSARY.md` — shared terminology
