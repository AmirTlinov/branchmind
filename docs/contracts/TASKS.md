# Contracts — MCP Command Surface (v3)

This file defines the active v3 command surface.

## Tool set

Exactly three MCP tools are advertised:

- `branch`
- `think`
- `merge`

Any other tool name is rejected with `UNKNOWN_TOOL`.

## Shared input schema

All tools accept:

- `workspace` (string, required)
- `markdown` (string, required)
- `max_chars` (integer, optional, range `1..=65536`, default `8192`)

`markdown` must be one strict fenced block:

```text
```bm
<verb> key=value key2="quoted value"
<optional body>
```
```

## Verbs by tool

### `branch`

- `main`
- `create`
- `list`
- `checkout`
- `delete`

### `think`

- `commit`
- `log`
- `show`
- `amend`
- `delete` (soft delete via tombstone commit)

### `merge`

- `into`

## Non-goals in v3

- No `tasks` portal surface.
- No `jobs`/`system`/`status`/`open` tools in the active MCP contract.
- No legacy alias tool names.
