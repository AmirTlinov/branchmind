# Contracts — Overview (v0)

This folder defines the **stable, versioned contracts** for the MCP tools exposed by this server.

## Scope

- Contracts describe **logical JSON payloads** for each tool.
- MCP transport (stdio, JSON-RPC envelope) is not repeated here.
- All tools are deterministic and side-effect only inside the local store, unless explicitly stated.

## Versioning

- Contract version: **v0** (breaking changes allowed until v1).
- Once v1 is declared, breaking changes require a major bump and migration notes.

## Naming constraints

Many MCP clients require tool names to match `^[a-zA-Z0-9_-]+$`.

## Contract index

- `TYPES.md` — common types, budgets, error model, response envelope
- `TASKS.md` — task execution surface (`tasks_*`)
- `MEMORY.md` — reasoning memory surface (branching, notes, graph, traces)
- `INTEGRATION.md` — how tasks and memory stay consistent (events, refs, conflicts)

Related:

- `../GLOSSARY.md` — shared terminology across domains
