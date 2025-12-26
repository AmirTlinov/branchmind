# Dependency Policy (draft)

The long-term goal is a low-dependency, high-performance Rust MCP server suitable for IDE embedding.

## Rules

- Prefer Rust standard library for core logic.
- Any external crate must be justified:
  - why it is needed,
  - alternatives considered,
  - risks (supply chain, size, maintenance),
  - how it is isolated (core vs adapter).

## Default allowance (until decided otherwise)

- Allow small, widely-audited crates for:
  - JSON (schema + serialization),
  - SQLite bindings (if an embedded DB is chosen),
  - RFC3339 time formatting (agent-facing timestamps),
  - MCP stdio protocol helpers.

If “0 deps strict” is required, replace these with in-house minimal implementations and document the trade-offs.

## Current usage (justified)

- `serde_json`: JSON parsing/serialization for MCP payloads and ops history snapshots (core logic
  remains std-only; JSON is isolated to adapters and persistence).
