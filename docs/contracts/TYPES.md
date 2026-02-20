# Contracts — Common Types & Error Model (v3)

## Tool response envelope

All three tools (`think`, `branch`, `merge`) return a typed JSON payload inside MCP `tools/call` text content.

Success shape:

```json
{
  "success": true,
  "intent": "think.log",
  "result": {},
  "warnings": [],
  "refs": []
}
```

Failure shape:

```json
{
  "success": false,
  "error": {
    "code": "INVALID_INPUT",
    "message": "...",
    "recovery": "..."
  },
  "warnings": [],
  "refs": []
}
```

Notes:

- `intent` is stable and deterministic for each tool verb.
- `warnings` are structured diagnostics; they do not imply success.
- `refs` are optional navigation references.

## Typed errors (v3)

Parser/surface errors:

- `UNKNOWN_TOOL` — tool is not in `{think, branch, merge}`.
- `UNKNOWN_ARG` — unknown top-level argument.
- `INVALID_INPUT` — malformed args, malformed markdown fence, malformed command line.
- `UNKNOWN_VERB` — verb is not supported by selected tool.
- `BUDGET_EXCEEDED` — `markdown` exceeds `max_chars`.

Runtime/storage errors:

- `UNKNOWN_ID` — requested branch/commit does not exist.
- `ALREADY_EXISTS` — attempted create conflicts with existing id.
- `MERGE_FAILED` — no source branches merged.
- `STORE_ERROR` — other deterministic store failures.

## Determinism rules

- No network I/O.
- No shell execution.
- Output depends only on input args + local store state.
- Parser and error mapping are fail-closed.
