# Quick Start

This repo is a deterministic, contract-first MCP server.

## 1) Verify toolchain and run checks

```bash
make check
```

## 2) Run the MCP server (DX defaults)

```bash
make run-mcp
```

Zero-arg invocation enables flagship DX defaults:

- shared proxy (session-scoped)
- default workspace (derived from repo root)
- repo-local store (`.agents/mcp/.branchmind/`)
- daily toolset
- workspace lock (guards against accidental cross-workspace calls)
- DX mode defaults (compact outputs + snapshot delta on by default)

## 2.1) Golden agent flow (PlanFS + strict reasoning checkpoints)

Use this minimal sequence inside MCP clients:

1. `system(op=quickstart args={portal:"tasks"})` — curated golden recipes only.
2. `tasks(op=call cmd=tasks.planfs.init args={...})` / `tasks.planfs.export` — keep plans in `docs/plans/**`.
3. `think(op=call cmd=think.trace.sequential.step args={...})` — record hypothesis→test→evidence→decision checkpoints.
4. `tasks(op=call cmd=tasks.snapshot args={view:"smart"})` — continue from the next actionable step.

Notes:

- Command/schema listing is golden-by-default (`system.cmd.list` / `system.schema.list`), use `mode="all"` for long-tail discovery.
- Knowledge namespace is removed by design; durable memory is repo-local skills + PlanFS files.

## OpenCode (recommended)

Configure the server as a local MCP backend and let BranchMind auto-configure everything:

```json
{
  "mcp": {
    "branchmind": {
      "type": "local",
      "command": ["/abs/path/to/bm_mcp"],
      "enabled": true,
      "timeout": 30000
    }
  }
}
```

## 3) Run the delegation runner (optional)

BranchMind (`bm_mcp`) is deterministic and does not execute arbitrary external programs.
Delegated work is modeled as `JOB-*` entities and executed out-of-process by the first-party runner:

```bash
cargo run -p bm_runner
```

To enable the `claude_code` executor (Claude Code CLI), configure the binary path:

```bash
# Option A: env
BM_CLAUDE_BIN=claude cargo run -p bm_runner

# Option B: flag
cargo run -p bm_runner -- --claude-bin claude
```

Notes:

- `bm_runner` uses repo-derived defaults (workspace + store) so the zero-arg invocation is the golden path.
- When no runner is live, jobs stay QUEUED; the portals (`status` / `jobs_radar`) surface a copy/paste bootstrap hint.

## 4) Where to look next

- Contracts: `docs/contracts/OVERVIEW.md`
- Architecture: `docs/architecture/ARCHITECTURE.md`
- Agent map: `AGENTS.md`

## Optional: Desktop Viewer (Tauri)

This repo ships an **optional read-only desktop viewer** under `apps/viewer-tauri/`.
It is intentionally **not** part of the Rust workspace (so `make check` stays fast and deterministic).

```bash
make viewer-install
make run-viewer
```

Notes:

- The viewer is **read-only** and opens stores via `SqliteStore::open_read_only`.
- On Linux/Wayland, `make run-viewer` automatically applies a safe WebKit setting
  (`WEBKIT_DISABLE_DMABUF_RENDERER=1`) and retries with X11 if Wayland disconnects.
- You can force X11 directly:

```bash
make run-viewer-x11
```

- Store discovery scans common roots; override with:

```bash
export BRANCHMIND_VIEWER_SCAN_ROOTS="/abs/path/one;/abs/path/two"
```
