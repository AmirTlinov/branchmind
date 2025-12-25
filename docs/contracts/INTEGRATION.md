# Contracts — Tasks ↔ Reasoning Integration (v0)

This document defines what makes the system a **single organism** rather than two unrelated toolsets.

## Core invariant

Every mutating task operation produces a durable reasoning event.

Stated differently:

> If the task state changed, the reasoning log must reflect it — without relying on agent discipline.

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
- Returned in `tasks_radar` and `tasks_resume`-like payloads.

## Sync event stream

Every mutating `tasks_*` response must include an append-only list of events:

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

Typical task-step event types (v0):

- `steps_added`
- `step_defined`
- `step_noted`
- `step_verified`
- `step_done`

## Conflicts lifecycle (discoverable)

When a merge conflict exists, it must be discoverable via a query:

- either a dedicated conflict listing tool, or
- conflict entities present in the graph with `status="conflict"`.

When resolved, the conflict must disappear from that listing/query.
