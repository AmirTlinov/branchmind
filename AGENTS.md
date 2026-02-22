# AGENTS.md — BranchMind Rust (Reasoning-only MCP)

## Mission

Build and maintain a deterministic local MCP server for agent thinking with exactly three tools:

- `branch`
- `think`
- `merge`

No tasks/jobs/runner subsystems. No compatibility layers for removed surfaces.

## Golden files

- `GOALS.md`
- `PHILOSOPHY.md`
- `docs/GLOSSARY.md`
- `docs/QUICK_START.md`
- `docs/contracts/OVERVIEW.md`
- `docs/contracts/V3_MCP_SURFACE.md`
- `docs/architecture/ARCHITECTURE.md`
- `docs/architecture/DEPENDENCIES.md`
- `REPO_RULES.md`

## Non-negotiable rules

### Determinism & safety

- No outbound network calls from `bm_mcp`.
- No arbitrary external program execution in `bm_mcp`.
- Do not print stored artifacts to stdout/stderr except explicit read responses.

### Contract discipline

- Behavior changes must update `docs/contracts/*` in the same slice.
- Unknown tool/verb/arg must stay fail-closed.
- Keep typed errors with recovery hints.

### Architecture boundaries

- `bm_core` is pure domain (std-only).
- `bm_storage` owns persistence/transactions.
- `bm_mcp` owns parsing, budgeting, and error mapping.

### Scope discipline

If code or docs are not required for `branch`/`think`/`merge`, remove them.

## Workspace map

```text
crates/
  core/      domain invariants
  storage/   sqlite adapter
  mcp/       stdio MCP server
docs/
  contracts/ active v3 contract
  architecture/ current architecture notes
```

## Quality gate

Before PR: run `make check`.
