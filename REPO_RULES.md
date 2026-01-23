# Repo Rules (BranchMind Rust)

These rules exist to keep the MCP server deterministic, cheap to maintain, and safe to evolve.

## Golden path

- `make check` (fmt-check + clippy + tests)
- `make run-mcp` (run the MCP server with DX defaults)

## Contract-first (non-negotiable)

- Any behavior change must update `docs/contracts/*` before (or alongside) code changes.
- Tests are the spec lock: add/extend contract and DX tests for new fields or new tools.

## Architecture boundaries (hexagonal)

- `bm_core` must remain pure (std-only, no storage, no transport, no OS I/O).
- Adapters:
  - `bm_storage`: persistence, transactions, indexing.
  - `bm_mcp`: schema validation, budgeting, typed error mapping.

## Determinism & safety

- No outbound network calls in runtime.
- All reads must be budgeted (`max_chars`, `max_bytes`, `limit`) with explicit truncation signals.
- All errors should be typed and include a recovery hint when possible.

## Code hygiene

- Formatting: `cargo fmt` (CI-enforced).
- Linting: `cargo clippy -- -D warnings` (CI-enforced).
- Prefer small focused modules; split "god-files" before they become unreviewable.

## Dependencies

- `bm_core` stays std-only.
- Any new crate must be justified in `docs/architecture/DEPENDENCIES.md`.
