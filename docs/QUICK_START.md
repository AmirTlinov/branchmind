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
- repo-local store (`.branchmind_rust/`)
- daily toolset
- workspace lock (guards against accidental cross-workspace calls)

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

## 3) Where to look next

- Contracts: `docs/contracts/OVERVIEW.md`
- Architecture: `docs/architecture/ARCHITECTURE.md`
- Agent map: `AGENTS.md`
