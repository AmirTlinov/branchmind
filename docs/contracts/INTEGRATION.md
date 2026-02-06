# Contracts — Tasks ↔ Reasoning Integration (v1)

This document defines what makes the system a **single organism** rather than two unrelated toolsets.

## Core invariant

Every mutating task operation produces a durable reasoning event.

Stated differently:

> If the task state changed, the reasoning log must reflect it — without relying on agent discipline.

Branching note:

- Task mutations always ingest into the **canonical** `reasoning_ref.branch` (and its `trace_doc`), regardless of any reasoning checkout/what-if branches.

## Human-authored note mirroring (`cmd=tasks.note` → `notes_doc`)

Some task operations carry meaningful human-authored text (progress notes).

To avoid losing that meaning between sessions, `cmd=tasks.note` must be mirrored into reasoning memory:

- The note content is appended as a `doc_entries.kind="note"` entry into the target's `notes_doc`.
- The mirror is written atomically with the task event (same transaction).
- The mirror always targets the **canonical** `reasoning_ref.branch` (never the checkout branch).

Recommended metadata (stored in `meta_json`) for the mirrored note:

- `source="tasks.note"`
- `task_id`, `step_id`, `path`
- `note_seq` (step note seq)
- `event_id` (the corresponding task event id)

## Reasoning reference

Each task/plan must be associated with a stable “reasoning reference”:

```json
{
  "branch": "task/TASK-001",
  "notes_doc": "notes",
  "graph_doc": "task-TASK-001-graph",
  "trace_doc": "task-TASK-001-trace"
}
```

Properties:

- Created lazily (on first need) and persisted.
- Survives server restarts.
- Returned in task resume/snapshot views (e.g. `cmd=tasks.snapshot`, `cmd=tasks.resume.super`).

## Task → graph projection

To make the system a **single organism**, task mutations project into the task's `graph_doc`
atomically and deterministically.

Projection rules:

- Node ids:
  - Task: `task:<TASK-###>`
  - Step: `step:<STEP-XXXXXXXX>`
- Node types: `task`, `step`.
- Edge type: `contains` (task → step, parent step → child step).
- Step node status is derived from step completion (`open` | `done`); task nodes omit status (v1 baseline).
- Deletions emit a `graph_node_delete` op and write tombstones for the node **and** all connected
  edges (no dangling edges in the effective view).
- Idempotency is guaranteed via `source_event_id = event_id + graph_key` (nodes/edges) and
  `event_id + node_delete + id` (deletes).
- Graph writes happen in the **same transaction** as the task event and trace ingestion,
  atomically and deterministically.

### Namespace (reserved)

- Task node id: `task:<TASK-###>`
- Step node id: `step:<STEP-XXXXXXXX>`

### Node types

- `task`
- `step`

### Edges

- `contains` — structural containment (task → step, parent step → child step)

### Guarantees

- Projection is written in the **same transaction** as the task event.
- `source_event_id` is derived from the task event id + graph key to ensure idempotency.
- MCP inputs/outputs are unchanged; this is an internal consistency rule.

## Sync event stream

Every mutating `cmd=tasks.*` operation must emit an append-only list of events (budgeted):

```json
{
  "events": [
    {
      "event_id": "evt_...",
      "ts": "2025-12-25T00:00:00Z",
      "ts_ms": 1735084800000,
      "workspace": "acme/repo",
      "task": "TASK-001",
      "path": "s:0.t:1.s:2",
      "type": "step_defined",
      "payload": { "title": "...", "criteria": ["..."] }
    }
  ]
}
```

Requirements:

- `event_id` is globally unique and idempotent.
- Events are written atomically with the task mutation.
- Events are ingested into the reasoning subsystem (trace + optional graph nodes) deterministically.

Typical task-step event types:

- `steps_added`
- `step_defined`
- `step_noted`
- `step_verified`
- `step_done`

Additional event types:

- `task_patched`
- `task_completed`
- `task_deleted`
- `task_node_added` / `task_node_defined` / `task_node_deleted`
- `step_reopened`
- `step_blocked` / `step_unblocked`
- `step_deleted`
- `plan_updated`
- `contract_updated`
- `evidence_captured`
- `undo_applied` / `redo_applied`

## Conflicts lifecycle (discoverable)

When a merge conflict exists, it must be discoverable via a query:

- either a dedicated conflict listing tool, or
- conflict entities present in the graph with `status="conflict"`.

When resolved, the conflict must disappear from that listing/query.
