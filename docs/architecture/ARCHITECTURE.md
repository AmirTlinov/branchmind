# Architecture (implemented)

This document describes the **current** code architecture and the invariants it is built to preserve.
It is written as a practical map for maintainers (humans and agents), not as a wish-list.

## Core design

The server is a deterministic state machine with two tightly-coupled domains:

1) **Task execution domain** (plans/tasks/steps/checkpoints)
2) **Reasoning domain** (append-only events, notes, diffs, merges, graph, traces)

They must share a single consistency boundary: a task mutation and its emitted event must be persisted atomically.

## Workspace shape (current)

BranchMind is a Cargo workspace with three crates:

- **`bm_core`** (`crates/core`): pure domain types + invariants (std-only).
- **`bm_storage`** (`crates/storage`): persistence adapter (single embedded SQLite store).
- **`bm_mcp`** (`crates/mcp`): MCP stdio JSON-RPC server + tool handlers (schema/budget discipline).

Dependency direction is strict:

- `bm_mcp` → `bm_storage` → `bm_core`
- `bm_mcp` → `bm_core`

`bm_core` must remain independent of transport and persistence.

## Boundaries (ports & adapters)

- **Core (pure):**
  - validates commands,
  - enforces invariants,
  - produces state transitions + events.
- **Storage adapter:**
  - persists state and events,
  - provides transactional guarantees,
  - indexes for fast lookups.
- **MCP adapter:**
  - validates input schemas,
  - enforces output budgets,
  - maps domain errors → typed MCP errors with recovery suggestions.

Dependency direction is strict: MCP → core ← storage.

## Consistency boundary (atomicity)

All mutating operations must be committed through the storage adapter such that:

- task mutations,
- emitted durable events (reasoning/event sink),
- reasoning reference updates (branch/doc pointers)

are persisted **atomically** (single transaction). This is a core “single organism” requirement: execution and reasoning
cannot drift apart.

## Performance posture

- All operations must be O(k) in the size of the requested output, not the size of the store.
- Reads are summary-first and budgeted; full exports are explicit and bounded.
- Mutations are single-transaction and idempotent where applicable.

## Testing strategy (current baseline)

- Contract tests for MCP response envelopes and tool list stability.
- Storage-level tests for critical flows (docs ingestion, think card commit, etc.).
- Invariant tests (expanding over time) for:
  - revision gating,
  - strict targeting,
  - checkpoint gating,
  - conflict lifecycle (create → discover → resolve → disappear).
- Budget tests for truncation semantics.

## Memory model (native-feeling UX)

The “native memory” user experience is specified separately as an explicit model and invariants:

- See `docs/architecture/MEMORY_MODEL.md`.
