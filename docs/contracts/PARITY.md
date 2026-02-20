# Contracts — Parity Matrix (v3)

v3 parity target is intentionally minimal and strict.

## Implemented surface

| Area | Status | Notes |
|---|---|---|
| MCP tools list | present | exactly `think`, `branch`, `merge` |
| Input schema | present | `workspace` + `markdown` required; `max_chars` optional |
| Markdown parser | present | strict single ` ```bm ... ``` ` block |
| Typed parser errors | present | `UNKNOWN_TOOL`, `UNKNOWN_ARG`, `INVALID_INPUT`, `UNKNOWN_VERB`, `BUDGET_EXCEEDED` |
| Typed runtime errors | present | `UNKNOWN_ID`, `ALREADY_EXISTS`, `MERGE_FAILED`, `STORE_ERROR` |

## Explicitly removed from parity target

- Legacy portal surfaces (`tasks`, `jobs`, `system`, etc.)
- Legacy alias tool names from v1/v2 migration eras
- Non-markdown command modes
