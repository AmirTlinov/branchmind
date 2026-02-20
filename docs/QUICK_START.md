# Quick Start (v3)

This repo currently ships a strict v3 MCP surface with exactly three tools:

- `branch`
- `think`
- `merge`

## 1) Verify

```bash
make check
```

## 2) Run server

```bash
make run-mcp
```

## 3) Smoke-check in your MCP client

### 3.1 tools/list must expose only 3 tools

Expect names:

- `branch`
- `think`
- `merge`

### 3.2 Create a branch

Tool: `branch`

```text
```bm
main
```
```

### 3.3 Append a thought commit

Tool: `think`

```text
```bm
commit branch=main commit=c1 message=Init body=Initial note
```
```

### 3.4 Read history

Tool: `think`

```text
```bm
log branch=main limit=20
```
```

### 3.5 Merge feature into main

Tool: `merge`

```text
```bm
into target=main from=feature strategy=squash
```
```

## Notes

- Inputs must be a single strict ` ```bm ... ``` ` fenced block.
- Unknown tools, verbs, or args fail closed with typed errors.
- Contracts: `docs/contracts/OVERVIEW.md` and `docs/contracts/V3_MCP_SURFACE.md`.
