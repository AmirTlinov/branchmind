# Runbook: Flagship Eval + BM-L1 (2-line portal)

This project treats the MCP surface as the product UX. The goal is to keep portal outputs
deterministic, low-noise, and copy/paste-safe for AI agents.

## Quick checks

- Portal flagship gates:
  - `cargo test -p bm_mcp --test flagship_eval`
- Full CI gate:
  - `cargo fmt --all -- --check`
  - `cargo test --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`

## BM-L1 invariants (daily)

- Default portal responses are 2 lines:
  - state line (untagged) includes `ref=<id>` (stable navigation handle)
  - command line is the primary next action
- Progressive disclosure is 3 lines:
  - state line
  - `tools/list toolset=...`
  - action command

## Navigation modes

- `tasks_snapshot refs=true` may emit extra `REFERENCE:` lines and/or a bounded
  `open ... max_chars=8000` jump line.
- `tasks_snapshot delta=true` may emit extra `REFERENCE:` lines for diff-oriented navigation.

## Changing portal output

If you change default BM-L1 output:

1. Update `docs/contracts/TASKS.md` (contracts-first).
2. Update `docs/architecture/NOISE_CONTRACT.md` if an invariant changes.
3. Update/extend tests:
   - `crates/mcp/tests/flagship_eval.rs`
   - `crates/mcp/tests/dx_dod.rs`
   - relevant portal/recovery tests
