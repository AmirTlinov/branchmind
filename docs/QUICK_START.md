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

### Rebuild / upgrade note (shared daemon UX)

When running in **shared mode**, a long-lived daemon process may survive local rebuilds.

- Starting a **newer** proxy binary automatically replaces a stale daemon (compat matches, but the
  daemon `build_time_ms` is older than the proxy).
- If you need an explicit escape hatch, call `system cmd=system.daemon.restart` via the `system`
  portal (one command; copy/paste safe).

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
