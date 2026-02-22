# Contracts — Parity Matrix (v3)

v3 parity target is intentionally minimal and strict.

## Implemented surface

| Area | Status | Notes |
|---|---|---|
| MCP tools list | present | exactly `think`, `branch`, `merge` |
| Input schema | present | `workspace` + `markdown` required |
| Markdown parser | present | strict single ` ```bm ... ``` ` block |
| Typed parser errors | present | `UNKNOWN_TOOL`, `UNKNOWN_ARG`, `INVALID_INPUT`, `UNKNOWN_VERB` |
| Typed runtime errors | present | `UNKNOWN_ID`, `ALREADY_EXISTS`, `MERGE_FAILED`, `STORE_ERROR` |

## Explicitly removed from parity target

- Removed non-current portal surfaces (`tasks`, `jobs`, `system`, etc.)
- Removed alias tool names outside the current v3 surface
- Non-markdown command modes
