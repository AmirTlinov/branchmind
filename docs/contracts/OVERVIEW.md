# Contracts — Overview (v1)

This folder defines the **stable, versioned contracts** for the MCP tools exposed by this server.

## Scope

- Contracts describe **logical JSON payloads** for each tool.
- MCP transport (stdio, JSON-RPC envelope) is not repeated here.
- All tools are deterministic.
- Side effects are limited to the local embedded store, except for explicitly documented
  local-only features (e.g. loopback viewer, optional runner autostart).

## Versioning

- Contract version: **v1** (breaking changes require a major bump).
- Once v1 is declared, breaking changes require a major bump and migration notes.

## Naming constraints

Many MCP clients require tool names to match `^[a-zA-Z0-9_-]+$`.

## Contract index

- `TYPES.md` — common types, budgets, error model, response envelope (v1)
- `V1_COMMANDS.md` — v1 cmd registry + golden ops
- `TASKS.md` — task execution surface (v1)
- `MEMORY.md` — reasoning memory surface (v1)
- `ANCHORS.md` — meaning-map anchors surface (v1)
- `DELEGATION.md` — delegation jobs (runner protocol + tracking)
- `SKILLS.md` — built-in behavior packs (`cmd=system.skill`)
- `INTEGRATION.md` — how tasks and memory stay consistent (events, refs, conflicts)
- `VIEWER.md` — optional local read-only HTTP viewer (non-MCP surface)

Related:

- `../GLOSSARY.md` — shared terminology across domains
