# Contracts — Overview (v3 MCP surface)

This folder defines the **stable, versioned contracts** for the MCP tools exposed by this server.

## Scope

- Contracts describe the MCP tool shapes and parser/runtime behavior.
- MCP transport (stdio, JSON-RPC envelope) is not repeated here.
- All tools are deterministic.
- Side effects are limited to the local embedded store, except for explicitly documented
  local-only features (e.g. optional runner autostart).

## Versioning

- Contract version: **v3 MCP surface**.
- Stable advertised tools: `think`, `branch`, `merge`.
- Inputs are markdown-only (` ```bm ... ``` ` fenced command block parser).
- Legacy tools are fail-closed (`UNKNOWN_TOOL`).

## Naming constraints

Many MCP clients require tool names to match `^[a-zA-Z0-9_-]+$`.

## Contract index

- `V3_MCP_SURFACE.md` — v3 tool list + strict fenced `bm` parser contract.

Related:

- `../GLOSSARY.md` — shared terminology across domains
