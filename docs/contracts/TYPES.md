# Contracts — Common Types & Error Model (v1)

This document defines shared types used across contracts.

## Response envelope (agent-first)

All tools return a single stable envelope:

```json
{
  "success": true,
  "intent": "tool_name_or_intent",
  "result": {},
  "refs": [],
  "actions": [
    {
      "action_id": "string",
      "priority": "high|medium|low",
      "tool": "tasks|think|...",
      "args": { "...": "..." },
      "why": "string",
      "risk": "string"
    }
  ],
  "warnings": [],
  "suggestions": [],
  "context": {},
  "error": null,
  "timestamp": "2025-12-25T00:00:00Z"
}
```

Notes:

- `actions[]` — единственный механизм “что дальше” (deterministic order).
- `suggestions[]` в v1 всегда `[]` (зарезервировано).

On failure: `success=false` and `error={code,message,recovery?}`.

### Output format — context-first (BM-L1)

AI agents pay a real token tax on repeated structural boilerplate. For daily usage, the server is **context-first**:
portal tools render a compact “tag-light” line protocol by default, with semantics defined once in docs/contracts
and enforced by tests (DX-DoD).

Design rule: **no json/context toggle for portals**. If a tool needs a structured payload, it should be exposed as a
separate explicit “full view” tool rather than teaching agents to switch formats.

BM-L1 line protocol (tag-light):

- Plain content line: stable, skimmable state (e.g. focus + next), **without a tag prefix**.
  - The `status` state line may include `build=<fingerprint>` (version + git sha + profile) so agents can detect stale servers/daemons.
- Untagged command lines: one or more copy/paste-ready next actions (tools/methods), placed after the state line.
  (We intentionally omit a `COMMAND:` prefix to avoid constant semantic noise.)
- Tagged utility lines:
  - `ERROR:` error (typed, actionable, with a recovery hint)
  - `WARNING:` warning / heads-up (typed, actionable, with a recovery hint)
  - `MORE:` continuation marker (pagination cursor / “more available”)
  - `REFERENCE:` reserved for rare navigation anchors (ids / doc seq). It may appear on delta outputs, on budget-truncated outputs, or when `refs=true` is requested; it should not appear by default.
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

- Callers may omit `workspace` in tool calls.
  - The server uses a deterministic default derived from the repo root directory name.
  - Override via `--workspace` / `BRANCHMIND_WORKSPACE`.
- Explicit `workspace` always wins (no silent re-targeting).

DX note (workspace lock, optional):

- The server may be configured to **lock** the default workspace (`--workspace-lock` / `BRANCHMIND_WORKSPACE_LOCK`).
- When workspace lock is enabled and a default workspace is set:
  - passing a different `workspace` becomes a typed error,
  - omitting `workspace` continues to work (defaults remain the golden path).

DX note (workspace allowlist, optional):

- The server may be configured with an allowlist of workspace ids (`BRANCHMIND_WORKSPACE_ALLOWLIST`).
- When an allowlist is set, any `workspace` outside the list becomes a typed error.

### AgentId (multi-agent lanes)

A short stable identifier for an agent instance when multiple agents work in the same workspace.

```json
{
  "type": "string",
  "minLength": 1,
  "maxLength": 64,
  "pattern": "^[A-Za-z0-9][A-Za-z0-9._-]*$"
}
```

Lane convention (tags + meta):

- `lane:shared` — shared lane (default).
- `lane:agent:<agent_id>` — **legacy** draft marker (kept for backwards compatibility).

DX note (default agent_id, optional):

- The server may be configured with a default agent id (`--agent-id` / `BRANCHMIND_AGENT_ID`).
- The default agent id is intended for **concurrency semantics** (e.g. step leases) and audit metadata.
- For meaning-mode memory tools, the server does **not** require an injected agent id; durable retrieval must not depend on it.
  (Implementation note: the MCP adapter may inject the default agent id only for `tasks_*` calls.)
- Durable memory must not depend on knowing a prior agent id:
  - stable resume is meaning-first (anchors + canon/pins),
  - draft expansion is an explicit opt-in for audit/sync (`include_drafts=true` / `all_lanes=true`; legacy lane tags are treated as drafts).

### AnchorId (meaning map)

A stable semantic identifier for an architecture area (boundary/component/contract/etc).

```json
{
  "type": "string",
  "minLength": 3,
  "maxLength": 66,
  "pattern": "^a:[a-z0-9][a-z0-9-]{0,63}$"
}
```

Notes:

- No hierarchy in the id: use explicit relations instead.
- IDs are stable across refactors (do not embed file paths).

### Visibility tags (draft vs canon)

Visibility is expressed via plain tags (lowercased):

- `v:canon` — canonical (shown in meaning-first anchor views).
- `v:draft` — draft (hidden by default; available via explicit expansion).

Legacy compatibility:

- `lane:agent:*` is treated as draft unless pinned or explicitly `v:canon`.

### PlanId / TaskId

Human-readable identifiers.

```json
{ "type": "string", "pattern": "^(PLAN|TASK)-[0-9]{3,}$" }
```

### JobId

Human-readable identifier for delegated work tracking.

```json
{ "type": "string", "pattern": "^JOB-[0-9]{3,}$" }
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

### StepPath

Human-oriented, index-based step addressing.

```json
{ "type": "string", "pattern": "^s:[0-9]+(\\.s:[0-9]+)*$" }
```

### GraphNodeId / GraphType / GraphRel

Graph identifiers are **validated and canonicalized** by the server. They are intentionally
string-based on the MCP surface, but treated as typed domain values internally.

Constraints:

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

### ConflictId

Graph merge conflicts are first-class entities with deterministic IDs.

```json
{ "type": "string", "pattern": "^CONFLICT-[0-9a-f]{32}$" }

## Task enums

### TaskStatus

```json
{ "type": "string", "enum": ["TODO", "ACTIVE", "PARKED", "DONE", "CANCELED"] }
```

### JobStatus

Delegation lifecycle state.

```json
{ "type": "string", "enum": ["QUEUED", "RUNNING", "DONE", "FAILED", "CANCELED"] }
```

### Priority

```json
{ "type": "string", "enum": ["LOW", "MEDIUM", "HIGH"] }
```

### Tags

```json
{ "type": "array", "items": { "type": "string" } }
```

Tag conventions:

- Pins:
  - `pinned` — force-show in relevance-first views (low-noise resume surface).
- Meaning map anchors:
  - `a:<slug>` — attach an artifact to an AnchorId (see above).
- Visibility (noise control without deleting):
  - `v:canon` — durable, reusable knowledge (decisions/evidence/tests/anchor definitions).
  - `v:draft` — exploratory notes (hidden by default in low-noise views; discoverable on demand).
- Lanes (legacy / optional):
  - `lane:shared`, `lane:agent:<agent_id>` — may be present on older artifacts or in explicit multi-agent workflows.

## Evidence artifacts

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

## Patch operation

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
- `context_budget` (tool-specific alias for `max_chars`, when supported)
- `max_bytes` (binary/text bytes)
- `max_lines` (diff/log)
- `limit`/`offset` (pagination)

All truncation must be explicit: `truncated=true` plus `budget.used_chars`/`budget.max_chars` where applicable.

Budget invariants:

- `used_chars <= max_chars` for budgeted payloads.
- `used_chars` counts the serialized payload **excluding** the `budget` field.
- If `max_chars` is too small to fit a minimal payload, the server clamps to a minimal safe value and emits `BUDGET_MIN_CLAMPED`.
- If the payload is reduced to a minimal signal, the server emits `BUDGET_MINIMAL`.

DX note (default budgets, optional):

- For selected **read-ish** tools, the server may apply a deterministic default `max_chars` / `context_budget`
  when the caller omits both. This prevents accidental “context blowups” in day-to-day usage while keeping
  callers fully in control once they specify budgets explicitly.

DX note (auto-escalation on truncation, optional):

- For selected read tools, when the caller omits budgets and the response emits `BUDGET_TRUNCATED` / `BUDGET_MINIMAL`,
  the server may retry the call a small, fixed number of times with a larger budget (still deterministic, still bounded by a hard cap),
  stopping early once truncation disappears.
  Explicit `max_chars` / `context_budget` always disables auto-escalation.

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
- `WORKSPACE_LOCKED`
- `WORKSPACE_NOT_ALLOWED`
- `PROJECT_GUARD_MISMATCH`
- `UNKNOWN_ID`
- `INVALID_NAME`
- `INVALID_INPUT`
- `REVISION_MISMATCH`
- `EXPECTED_TARGET_MISMATCH`
- `STRICT_TARGETING_REQUIRES_EXPECTED_TARGET_ID`
- `CHECKPOINTS_NOT_CONFIRMED`
- `STEP_LEASE_HELD`
- `STEP_LEASE_NOT_HELD`
- `CONFLICT`
- `BUDGET_EXCEEDED`

Notes:

- When `error.code="BUDGET_EXCEEDED"` and the server can identify the target command, it should attach
  an actions-first retry (e.g. `recover.budget.clamp::<cmd>`) that re-runs the same tool call with
  offending budget knobs clamped to the selected `budget_profile` caps.

Every error should include:

- `message` (human-readable)
- `recovery` (machine-oriented hint), when possible
- `suggestions[]` (executable), when possible
- `hints[]` (machine-readable), when helpful (best-effort; currently used primarily for `INVALID_INPUT`)

### `INVALID_INPUT` hints (best-effort)

When `error.code="INVALID_INPUT"`, the server may attach `error.hints[]` to help agents auto-repair
argument payloads with minimal retries.

Each hint is an object with a stable `kind` plus kind-specific fields.
Current kinds:

- `type`: `{ kind:"type", field, expected, items? }`
- `missing_required`: `{ kind:"missing_required", field }`
- `non_empty`: `{ kind:"non_empty", field }`
- `prefix`: `{ kind:"prefix", field, allowed:[...] }`
- `choose_one`: `{ kind:"choose_one", fields:[...], options:[{keep:[...], drop:[...]}...] }`
