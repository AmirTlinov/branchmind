# Contracts — Common Types & Error Model (v0)

This document defines shared types used across contracts.

## Response envelope (agent-first)

All tools return a single stable envelope:

```json
{
  "success": true,
  "intent": "tool_name_or_intent",
  "result": {},
  "warnings": [],
  "suggestions": [],
  "context": {},
  "error": null,
  "timestamp": "2025-12-25T00:00:00Z"
}
```

On failure: `success=false` and `error={code,message,recovery?}`.

### Suggestions (executable actions)

`suggestions[]` are server-emitted, tool-ready actions intended to be executed as-is:

```json
{
  "action": "call_tool",
  "target": "tasks_patch",
  "reason": "Fix missing checkpoint before closing",
  "priority": "high",
  "validated": true,
  "params": { "..." : "..." }
}
```

## Identifiers

### WorkspaceId

A stable namespace for a project/workspace (IDE-provided).

```json
{
  "type": "string",
  "minLength": 1,
  "maxLength": 128,
  "pattern": "^[A-Za-z0-9][A-Za-z0-9._/-]*$"
}
```

### PlanId / TaskId

Human-readable identifiers.

```json
{ "type": "string", "pattern": "^(PLAN|TASK)-[0-9]{3,}$" }
```

### StepId / TaskNodeId

Stable opaque identifiers.

```json
{ "type": "string", "pattern": "^(STEP|NODE)-[A-Za-z0-9]{8,}$" }
```

### StepPath (v0)

Human-oriented, index-based step addressing.

```json
{ "type": "string", "pattern": "^s:[0-9]+(\\.s:[0-9]+)*$" }
```

### GraphNodeId / GraphType / GraphRel (v0)

Graph identifiers are **validated and canonicalized** by the server. They are intentionally
string-based on the MCP surface, but treated as typed domain values internally.

Constraints (v0):

- `GraphNodeId`:
  - non-empty, max 256 chars
  - must not contain `|`
  - must not contain control characters
- `GraphType`:
  - non-empty, max 128 chars
  - must not contain control characters
- `GraphRel`:
  - non-empty, max 128 chars
  - must not contain `|`
  - must not contain control characters

### ConflictId (v0)

Graph merge conflicts are first-class entities with deterministic IDs.

```json
{ "type": "string", "pattern": "^CONFLICT-[0-9a-f]{32}$" }
```

## Optimistic concurrency

Every mutable entity carries a monotonic integer `revision`.

- Writes accept `expected_revision` (alias: `expected_version`).
- On mismatch, fail with `error.code="REVISION_MISMATCH"` and return recovery suggestions.

## Budgets (anti-context blowup)

Every potentially large output must accept at least one budget knob:

- `max_chars` (UTF-8 bytes, hard cap)
- `max_bytes` (binary/text bytes)
- `max_lines` (diff/log)
- `limit`/`offset` (pagination)

All truncation must be explicit: `truncated=true` plus `budget.used`/`budget.limit` where applicable.

## Error model

Errors are typed and actionable.

Recommended `error.code` values:

- `NOT_INITIALIZED`
- `UNKNOWN_WORKSPACE`
- `UNKNOWN_ID`
- `INVALID_NAME`
- `INVALID_INPUT`
- `REVISION_MISMATCH`
- `EXPECTED_TARGET_MISMATCH`
- `STRICT_TARGETING_REQUIRES_EXPECTED_TARGET_ID`
- `CHECKPOINTS_NOT_CONFIRMED`
- `CONFLICT`
- `BUDGET_EXCEEDED`

Every error should include:

- `message` (human-readable)
- `recovery` (machine-oriented hint), when possible
- `suggestions[]` (executable), when possible
