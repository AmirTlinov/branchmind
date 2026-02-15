[LEGEND]
TOOL_BUS = The contract for how tools are invoked and reported.

[CONTENT]
Contract: Tool bus v1

## Purpose
Define [TOOL_BUS]: how tools are invoked, how results are returned, and how failures are represented.

## Scope
- In scope: request/response envelopes, streaming semantics (if any), error taxonomy.
- Out of scope: tool-specific payload schemas (those are per-tool contracts).

## Interface
TODO: define request/response envelopes.

## Errors
TODO: define error taxonomy (typed errors, retryability, timeouts).

## Examples
```json
{"tool":"example_tool","input":{"q":"ping"}}
```
