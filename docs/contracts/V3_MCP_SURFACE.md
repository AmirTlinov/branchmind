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
- Unknown command arguments are rejected with `UNKNOWN_ARG` (fail-closed; no silent ignore).

## Tool verbs

- `branch`: `main`, `create`, `list`, `checkout`, `delete`
- `think`: `commit`, `log`, `show`, `amend`, `delete`
- `merge`: `into`

### Verb argument contract (strict)

- `branch.main`: _(no args)_
- `branch.create`: `branch`, optional one of (`from` | `parent`)  
  (`from` and `parent` together are invalid)
- `branch.list`: optional `limit`, `offset`
- `branch.checkout`: `branch`
- `branch.delete`: `branch`

- `think.commit`: `branch`, `commit`, `message`, optional `body`, `parent`
- `think.log`: `branch`, optional `limit`, `offset`, `from`
- `think.show`: `commit`
- `think.amend`: `commit`, `new_commit`, optional `branch`, `message`, `body`
- `think.delete`: `commit`, `new_commit`, optional `branch`, `message`, `body`

- `merge.into`: `target`, `from`, optional `strategy`, `summary`, `message`, `body`

## Error model (typed)

- `UNKNOWN_TOOL`
- `UNKNOWN_ARG`
- `INVALID_INPUT`
- `UNKNOWN_VERB`
- `UNKNOWN_ID`
- `ALREADY_EXISTS`
- `MERGE_FAILED`
- `STORE_ERROR`
