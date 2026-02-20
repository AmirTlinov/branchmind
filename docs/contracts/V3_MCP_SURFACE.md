# V3 MCP surface (markdown-only)

## Advertised tools

`tools/list` returns exactly:

- `think`
- `branch`
- `merge`

Legacy tool names are fail-closed with `UNKNOWN_TOOL`.

## Shared input contract

Input is a JSON object:

- `workspace` (string, required)
- `markdown` (string, required)
- `max_chars` (integer, optional, `1..=65536`, default `8192`)

Unknown top-level keys are rejected with `UNKNOWN_ARG`.

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
- Closing fence is exactly ```` ``` ```` on its own line.
- First block line is required and parsed as command line.
- Tokens after verb must be `key=value`.
- Duplicate keys are rejected.
- Unknown verbs are rejected with `UNKNOWN_VERB`.

## Tool verbs

- `branch`: `main`, `create`, `list`, `checkout`, `delete`
- `think`: `commit`, `log`, `show`, `amend`, `delete`
- `merge`: `into`

## Error model (typed)

- `UNKNOWN_TOOL`
- `UNKNOWN_ARG`
- `INVALID_INPUT`
- `UNKNOWN_VERB`
- `BUDGET_EXCEEDED`
- `UNKNOWN_ID`
- `ALREADY_EXISTS`
- `MERGE_FAILED`
- `STORE_ERROR`
