# Architecture (documentation-first)

This document describes the intended architecture without locking implementation details prematurely.

## Core design

The server is a deterministic state machine with two tightly-coupled domains:

1) **Task execution domain** (plans/tasks/steps/checkpoints)
2) **Reasoning domain** (append-only events, notes, diffs, merges, graph, traces)

They must share a single consistency boundary: a task mutation and its emitted event must be persisted atomically.

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

## Performance posture

- All operations must be O(k) in the size of the requested output, not the size of the store.
- Reads are summary-first and budgeted; full exports are explicit and bounded.
- Mutations are single-transaction and idempotent where applicable.

## Testing strategy (minimum)

- Contract tests for schemas and response shapes.
- Invariant tests for:
  - revision gating,
  - strict targeting,
  - checkpoint gating,
  - conflict lifecycle (create → discover → resolve → disappear).
- Budget tests for truncation semantics.

