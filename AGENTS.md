# AGENTS.md — BranchMind Rust (Unified Task+Reasoning MCP)

This file is the project map and the operating rules for AI agents developing this repository.

## Mission

Build a **single MCP server** that makes AI agents dramatically better at:

- planning and executing long, complex tasks,
- preserving decisions, evidence, and context across sessions,
- exploring alternatives via branching and merging,
- resuming work fast with low-noise summaries.

No GUI/TUI. MCP-only. Rust-first. Deterministic.

## Golden files (read first)

- `GOALS.md` — explicit goals and non-goals
- `PHILOSOPHY.md` — philosophy (no implementation hardcoding)
- `docs/GLOSSARY.md` — shared terminology (execution + reasoning)
- `docs/contracts/OVERVIEW.md` — contract entrypoint
- `docs/architecture/ARCHITECTURE.md` — boundaries and dependency direction
- `docs/architecture/PLAN.md` — staged implementation milestones
- `docs/architecture/LEGACY_PITFALLS.md` — what not to repeat from prior implementations

## Rules (non-negotiable)

### Determinism & safety

- No network calls.
- No code execution.
- Never log committed artifacts to stdout/stderr outside explicit “read” tools.
- Treat all stored artifacts as potentially sensitive; do not auto-ingest env/config.

### MCP contract discipline

- Contract-first: update `docs/contracts/*` before (or alongside) any behavior change.
- Every tool must have:
  - schema-stable inputs/outputs,
  - explicit budget knobs (`max_bytes`, `max_chars`, `limit`, etc),
  - typed errors with recovery hints.

### “Single organism” integration (core requirement)

- Every mutating `tasks_*` operation must emit a durable event into the reasoning subsystem.
- A task must have a stable reasoning reference (`notes_doc`, `graph_doc`, `trace_doc`) created lazily and persisted.
- Merge conflicts must be discoverable (`status="conflict"` or an equivalent query) and resolvable via an explicit tool.

### Architecture boundaries (hexagonal)

- Domain core contains invariants and pure logic; it must not depend on:
  - MCP transport,
  - storage engines,
  - filesystem/OS I/O.
- Adapters implement ports:
  - MCP adapter: request validation, schema, budgeting, error mapping.
  - Storage adapter: persistence, transactions, indexing.

### Rust + dependency budget

- Prefer std-only components for core logic.
- Any external crate must be justified in `docs/architecture/DEPENDENCIES.md` (why it is needed, alternatives considered, risk).

## Project map (expected)

> This repo is currently documentation-first. Code layout below is the intended destination.

```text
docs/
  contracts/               MCP surface (schemas + semantics)
  architecture/            boundaries, storage, test strategy
crates/
  core/                    pure domain (tasks + reasoning primitives)
  storage/                 persistence adapter (single embedded store)
  mcp/                     MCP server (stdio) adapter
```

## Primary code entrypoints (expected)

> These files do not exist yet; they are the intended “anchors” for navigation once implementation starts.

- MCP server main: `crates/mcp/src/main.rs`
- Domain core root: `crates/core/src/lib.rs`
- Storage adapter root: `crates/storage/src/lib.rs`

## Blockers (must be decided explicitly)

- Tool naming: whether we expose two families (`tasks_*` and `branchmind_*`) or a unified prefix.
- Storage boundary: single embedded store vs. separated stores (must still be atomic for task+event writes).
- Dependency policy interpretation: “0 deps” strict vs. minimal audited deps.

## Practical development workflow (once code exists)

- Formatting: `cargo fmt --all`
- Lints: `cargo clippy --all-targets -- -D warnings`
- Tests: `cargo test --all`

## Aliases (quick navigation)

- Glossary: `docs/GLOSSARY.md`
- Contracts entrypoint: `docs/contracts/OVERVIEW.md`
- Types/errors: `docs/contracts/TYPES.md`
- Task API: `docs/contracts/TASKS.md`
- Reasoning/memory API: `docs/contracts/MEMORY.md`
- Integration contract: `docs/contracts/INTEGRATION.md`
- Architecture: `docs/architecture/ARCHITECTURE.md`
- Plan: `docs/architecture/PLAN.md`
- UX rules: `docs/architecture/AGENT_UX.md`
