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
  - MCP stdio protocol helpers.

If “0 deps strict” is required, replace these with in-house minimal implementations and document the trade-offs.

