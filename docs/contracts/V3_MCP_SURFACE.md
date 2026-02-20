# V3 MCP surface (markdown-only)

## Advertised tools

`tools/list` returns exactly:

- `think`
- `branch`
- `merge`

Any legacy tool name is rejected with `UNKNOWN_TOOL`.

## Input contract (all three tools)

Input is a JSON object:

- `workspace` (string, required)
- `markdown` (string, required)
- `max_chars` (integer, optional, `1..=65536`, default `8192`)

## Strict parser contract

`markdown` must contain exactly one fenced `bm` block:

```text
```bm
<verb> key=value key2="quoted value"
<optional body lines>
```
```

Rules:

- No non-whitespace text before/after the fenced block.
- Opening fence is exactly ```` ```bm ````.
- Closing fence is exactly ```` ``` ````.
- First line in the block is required and parsed as command line.
- Tokens after verb must be `key=value`; duplicate keys are rejected.
- Unknown verbs are rejected with `UNKNOWN_VERB`.

## Error model (typed)

- `UNKNOWN_TOOL` — tool is not in `{think, branch, merge}`.
- `INVALID_INPUT` — malformed args/markdown/command line.
- `UNKNOWN_ARG` — top-level tool argument is not allowed.
- `UNKNOWN_VERB` — verb is not valid for selected tool.
- `BUDGET_EXCEEDED` — markdown exceeds `max_chars`.
- `UNKNOWN_ID` / `ALREADY_EXISTS` / `STORE_ERROR` — runtime/store failures.
