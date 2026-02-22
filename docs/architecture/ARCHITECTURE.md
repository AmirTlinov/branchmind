# Architecture (current)

BranchMind is a reasoning-only Rust workspace with three crates:

- `bm_core` — pure domain invariants (`ThoughtBranch`, `ThoughtCommit`, `MergeRecord`, identifiers)
- `bm_storage` — embedded SQLite adapter for branch/commit/merge persistence
- `bm_mcp` — MCP stdio adapter exposing `branch`, `think`, `merge`

## Dependency direction

- `bm_mcp` -> `bm_storage` -> `bm_core`
- `bm_mcp` -> `bm_core`

`bm_core` stays transport/storage agnostic.

## Runtime boundary

- MCP transport is JSON-RPC over stdio.
- Optional shared/daemon modes are local transport helpers only.
- No network I/O.

## Storage model

Minimal v3 schema only:

- `workspaces`
- `branches`
- `branch_checkout`
- `commits`
- `merge_records`
- `workspace_state`

Legacy schemas are rejected with `RESET_REQUIRED`.

## Tool contract

Active tool surface is fixed by `docs/contracts/V3_MCP_SURFACE.md`:

- `branch`
- `think`
- `merge`

Everything else is out of scope.
