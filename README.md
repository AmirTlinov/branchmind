# BranchMind (v3 MCP surface)

BranchMind is a deterministic Rust MCP server with a minimal tool surface:

- `branch`
- `think`
- `merge`

All three tools use the same markdown command input (` ```bm ... ``` `).

## What v3 guarantees

- strict, fail-closed parser
- typed errors with recovery hints
- local embedded-store only (no network I/O)
- deterministic behavior from args + persisted state

## Quick start

```bash
make check
make run-mcp
```

## MCP client config (example)

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

## Minimal workflow

1) Create main branch:

```json
{
  "name": "branch",
  "arguments": {
    "workspace": "demo",
    "markdown": "```bm\nmain\n```"
  }
}
```

2) Write thought commit:

```json
{
  "name": "think",
  "arguments": {
    "workspace": "demo",
    "markdown": "```bm\ncommit branch=main commit=c1 message=Start body=Initial\n```"
  }
}
```

3) Merge a feature branch into main:

```json
{
  "name": "merge",
  "arguments": {
    "workspace": "demo",
    "markdown": "```bm\ninto target=main from=feature strategy=squash\n```"
  }
}
```

## Docs

- Contracts entrypoint: `docs/contracts/OVERVIEW.md`
- v3 surface contract: `docs/contracts/V3_MCP_SURFACE.md`
- Developer quick start: `docs/QUICK_START.md`
- Architecture: `docs/architecture/ARCHITECTURE.md`
