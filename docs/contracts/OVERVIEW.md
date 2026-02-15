# Contracts — Overview (v1)

This folder defines the **stable, versioned contracts** for the MCP tools exposed by this server.

## Scope

- Contracts describe **logical JSON payloads** for each tool.
- MCP transport (stdio, JSON-RPC envelope) is not repeated here.
- All tools are deterministic.
- Side effects are limited to the local embedded store, except for explicitly documented
  local-only features (e.g. optional runner autostart).

## Versioning

- Contract version: **v1** (breaking changes require a major bump).
- Once v1 is declared, breaking changes require a major bump and migration notes.
- Server package version may move independently (current rollout: `2.0.0`) while preserving v1
  contract docs as the stable surface.

## Naming constraints

Many MCP clients require tool names to match `^[a-zA-Z0-9_-]+$`.

## Contract index

- `TYPES.md` — common types, budgets, error model, response envelope (v1)
- `V1_COMMANDS.md` — v1 cmd registry + golden ops
- `V1_MIGRATION.md` — old tool names → cmd
- `TASKS.md` — legacy task surface (v0)
- `MEMORY.md` — reasoning memory surface (branching, notes, graph, traces)
- `ANCHORS.md` — meaning-map anchors surface (architecture-scoped memory)
- `DELEGATION.md` — delegation jobs (runner protocol + tracking)
- `SKILLS.md` — built-in behavior packs (`skill` tool)
- `INTEGRATION.md` — how tasks and memory stay consistent (events, refs, conflicts)
- `PARITY.md` — parity target with apply_task + branchmind tool surfaces
- `VIEWER.md` — local-only Tauri viewer IPC contract (not part of MCP surface)

Related:

- `../GLOSSARY.md` — shared terminology across domains
