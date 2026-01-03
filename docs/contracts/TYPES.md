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

### Output format — context-first (BM-L1)

AI agents pay a real token tax on repeated structural boilerplate. For daily usage, the server is **context-first**:
portal tools render a compact “tag-light” line protocol by default, with semantics defined once in docs/contracts
and enforced by tests (DX-DoD).

Design rule: **no json/context toggle for portals**. If a tool needs a structured payload, it should be exposed as a
separate explicit “full view” tool rather than teaching agents to switch formats.

BM-L1 line protocol (tag-light):

- Plain content line: stable, skimmable state (e.g. focus + next), **without a tag prefix**.
- Untagged command lines: one or more copy/paste-ready next actions (tools/methods), placed after the state line.
  (We intentionally omit a `COMMAND:` prefix to avoid constant semantic noise.)
- Tagged utility lines:
  - `ERROR:` error (typed, actionable, with a recovery hint)
  - `WARNING:` warning / heads-up (typed, actionable, with a recovery hint)
  - `MORE:` continuation marker (pagination cursor / “more available”)
  - `REFERENCE:` reserved for rare anchors (external evidence, doc seq, ids) and should not appear by default.
- `WATERMARK:` is reserved for future use; it does not appear in normal BM-L1 outputs.

In BM-L1 mode, the server may intentionally leave `warnings[]` / `suggestions[]` empty because those are
rendered into `WARNING:` lines and plain command lines to keep the response envelope low-noise.

Transport note (MCP text output):

- When a tool renders BM-L1, the MCP server may return the **raw line-protocol text** directly as the tool `text`
  content to avoid wasting tokens on a JSON envelope.

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

In addition to `call_tool`, the server may emit a `call_method` suggestion for MCP-native methods
(most commonly progressive disclosure via `tools/list`):

```json
{
  "action": "call_method",
  "method": "tools/list",
  "reason": "Reveal full tool surface for recovery",
  "priority": "high",
  "validated": true,
  "params": { "toolset": "full" }
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

DX note (default workspace):

- Portal tools may allow omitting `workspace` when the server is configured with a default workspace (`--workspace` / `BRANCHMIND_WORKSPACE`).
- Explicit `workspace` always wins (no silent re-targeting).

### PlanId / TaskId

Human-readable identifiers.

```json
{ "type": "string", "pattern": "^(PLAN|TASK)-[0-9]{3,}$" }
```

### QualifiedId

Workspace-scoped identifier for stable cross-workspace references.

Format: `<workspace>:<id>` (e.g. `acme:PLAN-012`).

### TargetRef (plan/task selector)

Unified target input accepted across task and reasoning tools.

```json
{
  "type": "object",
  "properties": {
    "id": { "type": "string", "pattern": "^(PLAN|TASK)-[0-9]{3,}$" },
    "kind": { "type": "string", "enum": ["plan", "task"] }
  },
  "required": ["id"]
}
```

When a tool accepts `target`, it may be provided as a string (`"TASK-001"`) or `TargetRef`.

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

## Task enums (v0.2)

### TaskStatus

```json
{ "type": "string", "enum": ["TODO", "ACTIVE", "DONE"] }
```

### Priority

```json
{ "type": "string", "enum": ["LOW", "MEDIUM", "HIGH"] }
```

### Tags

```json
{ "type": "array", "items": { "type": "string" } }
```

## Evidence artifacts (v0.2)

Artifacts are bounded and sanitized on output.

```json
{
  "kind": "cmd_output|diff|url",
  "command": "string?",
  "stdout": "string?",
  "stderr": "string?",
  "exit_code": 0,
  "diff": "string?",
  "content": "string?",
  "url": "string?",
  "external_uri": "string?",
  "meta": {}
}
```

## Patch operation (v0.2)

```json
{
  "op": "set|unset|append|remove",
  "field": "string",
  "value": "any?"
}
```
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

All truncation must be explicit: `truncated=true` plus `budget.used_chars`/`budget.max_chars` where applicable.

Budget invariants:

- `used_chars <= max_chars` for budgeted payloads.
- `used_chars` counts the serialized payload **excluding** the `budget` field.
- If `max_chars` is too small to fit a minimal payload, the server clamps to a minimal safe value and emits `BUDGET_MIN_CLAMPED`.
- If the payload is reduced to a minimal signal, the server emits `BUDGET_MINIMAL`.

## Redaction (best-effort, safe-by-default)

Outputs are scrubbed for likely secrets:

- key names containing `token`, `secret`, `password`, `api_key`, `authorization`, `bearer`,
- token-like prefixes (`ghp_`, `github_pat_`, `sk-`),
- `Bearer` tokens and query-string secrets (`token=...`, `api_key=...`, etc).

Redaction replaces sensitive values with `<redacted>` without changing the schema.

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
