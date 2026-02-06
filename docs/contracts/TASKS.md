# Contracts — Tasks (v1)

BranchMind task execution UX is exposed via a **single portal tool**:

- Tool: `tasks`
- Operations: `op="call" + cmd="tasks.*"` (long-tail, schema-discoverable)
- Optional convenience: `op="<alias>"` for a small set of golden ops (discoverable via `tools/list`)

This document defines the **v1 semantics** of the task domain (plans/tasks/steps/checkpoints),
independent of MCP transport.

---

## Unified portal envelope

All task operations use the portal input shape:

```json
{
  "workspace": "string?",
  "op": "call|<golden-op>",
  "cmd": "tasks.* (required only for op=call)",
  "args": { "...": "..." },
  "budget_profile": "portal|default|audit",
  "view": "compact|smart|audit"
}
```

Notes:

- `workspace` may be omitted when a deterministic default workspace is configured (DX mode).
- Outputs are budgeted by default; truncation must be explicit (see `TYPES.md`).

---

## Conceptual model

- **Plan** (`PLAN-###`): a top-level container with a checklist of steps.
- **Task** (`TASK-###`): a work item under a plan; contains a tree of steps.
- **Step** (`STEP-...`): smallest checkpointable unit inside a task.
- **Checkpoint**: a gate that must be confirmed before closing (e.g. `gate|tests|security|docs`).
- **Revision**: monotonic integer for optimistic concurrency (`expected_revision`).
- **Focus**: a convenience pointer for resumption; never the source of truth for writes.

Core invariants (MUST):

1) **No silent mis-target on writes**
   - Prefer explicit `task` + `step_id` targeting.
   - “Focus-based” default targeting is convenience only and must be visible in outputs.

2) **Checkpoint-gated completion**
   - Closing a step is blocked until required checkpoints are confirmed.
   - All “what next” guidance is expressed via `actions[]` (actions-first).

---

## Key commands (semantic index)

Use `system` → `schema.get(cmd)` for exact schemas and examples.

### `tasks.execute.next` (gold, NextEngine)

Purpose: return the **next action** (and backup action) for the current workspace/task focus.

- Used by `status` and by task flows to keep “what next” consistent.
- Deterministic: no randomness; same inputs → same outputs.

### `tasks.plan.create` (alias: `plan.create`)

Create a plan or a task.

Example (minimal, placeholders):

```json
{
  "op": "plan.create",
  "args": {
    "title": "<title>",
    "description": "<optional>",
    "kind": "task|plan",
    "parent": "PLAN-123?"
  }
}
```

### `tasks.plan.decompose` (alias: `plan.decompose`)

Decompose a task into steps (bounded, deterministic ordering).

### `tasks.snapshot` (call-only)

Return the **resume snapshot** for the current focus (task + next steps + refs + optional packs).

- `view=compact` should be BM‑L1: ref-first + 1 primary next action + 1 backup action.
- `view=smart|audit` may include more context, bounded by budgets.

### `tasks.step.close` (alias: `step.close`)

Close a step, enforcing checkpoint gates.

Expected UX:

- If checkpoints are missing → typed error (e.g. `CHECKPOINTS_NOT_CONFIRMED`) + recovery `actions[]`.
- If proof is required → typed error `PROOF_REQUIRED` + recovery `actions[]`.

### `tasks.evidence.capture` (alias: `evidence.capture`)

Attach durable evidence to a task/step:

- proofs (`CMD:` / `LINK:` / `FILE:` receipts),
- notes,
- external artifact references.

This is the canonical bridge between “I did the work” and “future me can verify it quickly”.

---

## Macros (workflow accelerators)

Macros are still normal `cmd=tasks.*` commands. They are typically **call-only** (not advertised as golden ops)
to keep `tools/list` low-noise under `toolset=core`.

Common ones:

- `tasks.macro.start` — create a task with a ready-to-run step structure and return a resume capsule
- `tasks.macro.close.step` — confirm checkpoints + close step + return resume (fast “done” loop)
- `tasks.macro.delegate` — create a task slice + create a `JOB-*` for delegated execution
- `tasks.macro.finish` — finalize the task (often after all steps are closed)

---

## Integration requirement (single organism)

Every mutating task operation MUST emit a durable event into the reasoning subsystem,
and tasks MUST have stable reasoning refs (`notes_doc`, `graph_doc`, `trace_doc`) created lazily.

See `INTEGRATION.md` for the normative contract.

