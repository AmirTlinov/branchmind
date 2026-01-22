# Contributing

## Local workflow

Prereqs: Rust toolchain (see `rust-toolchain.toml`).

```bash
make check
```

Run the MCP server (DX auto defaults):

```bash
make run-mcp
```

## Adding or changing a tool

1. Update the contract in `docs/contracts/*`.
2. Implement the handler under `crates/mcp/src/tools/`.
3. Enforce budgets and return typed errors with recovery hints.
4. Add/extend tests (treat tests as the spec lock).
5. Run `make check`.

## Adding a dependency

1. Prefer std-only changes in `bm_core`.
2. If a dependency is necessary, document it in `docs/architecture/DEPENDENCIES.md`:
   - why it is needed,
   - alternatives considered,
   - risks,
   - containment (which crate and why).
