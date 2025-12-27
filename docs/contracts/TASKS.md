# Contracts — Task Execution Surface (v0)

This server provides a task execution API designed for AI agents:

- explicit targeting (no silent mis-target),
- checkpoint-gated completion,
- optimistic concurrency,
- “one screen → one truth” summaries.

## Conceptual model

- **Plan** (`PLAN-###`): top-level container and checklist (contract + steps).
- **Task** (`TASK-###`): work item under a plan, with a tree of **steps**.
- **Step**: node inside a task; each step has checkpoints and may contain a sub-plan.

## Required UX properties

- `radar` must provide: **Now / Why / Verify / Next / Blockers** under a bounded payload.
- `handoff` must provide: what’s done / what remains / risks, plus the radar core.
- Completion is gated: steps cannot be marked done without confirmed checkpoints.

## Step selectors (v0)

Steps can be addressed by:

- `step_id` (stable, preferred for safety),
- `path` (index-based convenience, e.g. `s:0.s:2`).

Writes must never silently change targets; use explicit ids where possible.

## Workspace scoping (MUST)

All stateful tools in this family operate inside an explicit `workspace` (a stable IDE-provided identifier).

- `workspace` is required in v0 to avoid implicit context and silent cross-project writes.

## Payload budgets (v0)

- `tasks_context`, `tasks_delta`, `tasks_radar`, `tasks_handoff`, `tasks_context_pack`, `tasks_resume_pack`, `tasks_resume_super` accept optional `max_chars`.
- When `max_chars` is provided, responses include `budget` with `{ max_chars, used_chars, truncated }`.
- If the payload must shrink past normal truncation, the response may drop optional fields and return a minimal signal plus warnings.

## Tool family

> Names are provisional. The default assumption is `tasks_*` for compatibility with prior ecosystems.

Minimum required tools (v0 MVP):

- `tasks_create` (plan/task)
- `tasks_context` (list)
- `tasks_edit` (plan/task meta)
- `tasks_focus_get` / `tasks_focus_set` / `tasks_focus_clear` (convenience focus, workspace-scoped)
- `tasks_decompose` (add steps)
- `tasks_define` (define step title/criteria/tests/blockers)
- `tasks_note` (progress notes)
- `tasks_verify` (confirm checkpoints)
- `tasks_done` (close step)
- `tasks_close_step` (atomic verify + close)
- `tasks_radar` (one-screen snapshot)
- `tasks_delta` (event/ops stream)

Parity tool set (v0.2 target):

- Lifecycle/meta: `tasks_complete`, `tasks_patch`, `tasks_delete`, `tasks_progress`, `tasks_block`
- Plans/contracts: `tasks_plan`, `tasks_contract`
- Batch/ops: `tasks_batch`, `tasks_history`, `tasks_undo`, `tasks_redo`
- Evidence: `tasks_evidence_capture`
- Context views: `tasks_resume`, `tasks_context_pack`, `tasks_mirror`, `tasks_handoff`, `tasks_lint`
- Subplans: `tasks_task_add`, `tasks_task_define`, `tasks_task_delete`
- Templates: `tasks_templates_list`, `tasks_scaffold`
- Utilities: `tasks_storage`

For full intent surfaces and semantics, use this project’s contracts as the source of truth (do not import legacy behavior blindly).

## Core lifecycle (v0.2)

### `tasks_create`

Create a plan or task.

Input: `{ workspace, kind?, parent?, title, description?, contract?, contract_data?, steps? }`

Semantics:

- `kind` defaults to `task` when `parent` is provided; otherwise `plan`.
- `steps` are allowed only for task creation.
- When `steps` are provided:
  - `steps[].title` and `steps[].success_criteria` are required.
  - `steps[].tests` / `steps[].blockers` are optional and applied via `tasks_define`.
  - Response includes `steps` and `events` for the create + step operations.

### `tasks_complete`

Sets plan/task status.

Input: `{ workspace, task, status? }` where `status` is `TODO|ACTIVE|DONE` (default `DONE`).

Semantics:

- Uses optimistic concurrency (`expected_revision`).
- For tasks, completion requires all steps to be completed and checkpoints confirmed.

### `tasks_edit`

Edit plan/task metadata.

Input (union):

- Plan: `{ workspace, task, title?, description?, context?, priority?, tags?, depends_on?, contract?, contract_data? }`
- Task: `{ workspace, task, title?, description?, context?, priority?, new_domain?, tags?, depends_on? }`

Semantics:

- Applies an atomic patch and emits a single event.
- `new_domain` maps to task `domain`.

### `tasks_close_task`

Atomic close: optional patches + completion in one call.

Input: `{ workspace, task, apply?, patches?, expected_revision? }`

Semantics:

- `apply=false` returns a dry-run diff.
- `apply=true` applies patches (if any) and completes the task in a single transaction.
- Must fail fast on `REVISION_MISMATCH` or unmet checkpoints.

### `tasks_patch`

Diff-oriented updates for task detail, step, or task node.

Input:

```json
{
  "workspace": "acme/repo",
  "task": "TASK-001",
  "kind": "task_detail|step|task",
  "path": "s:0",
  "step_id": "STEP-XXXX",
  "task_node_id": "NODE-XXXX",
  "ops": [ { "op": "set|unset|append|remove", "field": "title", "value": "..." } ]
}
```

Semantics:

- `kind=task_detail` targets plan/task root metadata.
- `kind=step` targets a step (by `path` or `step_id`).
- `kind=task` targets a task node inside a step plan (path `s:0.t:1`).
- All mutations emit events and are checkpoint-aware where applicable.

### `tasks_progress`

Toggle step completion.

Input: `{ workspace, task, path|step_id, completed, force? }`

Semantics:

- `completed=true` behaves like `tasks_done` (checkpoint gated).
- `completed=false` reopens the step and clears completion timestamp.

### `tasks_block`

Block/unblock a step.

Input: `{ workspace, task, path|step_id, blocked?, reason? }`

Semantics:

- Sets `blocked` + optional reason on the step.
- Emits `step_blocked` or `step_unblocked`.

### `tasks_delete`

Deletes a plan/task or a step by selector.

Input: `{ workspace, task, path?|step_id? }`

Semantics:

- Deleting a plan/task removes the root entity and its steps.
- Deleting a step removes the step subtree.

## Plans & contracts (v0.2)

### `tasks_plan`

Plan checklist update.

Input: `{ workspace, plan, steps?, current?, doc?, advance? }`

Semantics:

- `steps` replaces the checklist.
- `advance=true` increments `current` by 1.

### `tasks_contract`

Set or clear a plan contract.

Input: `{ workspace, plan, current?, contract_data?, clear? }`

Semantics:

- `clear=true` removes contract text and structured data.

## Batch & ops (v0.2)

### `tasks_batch`

Run multiple task operations atomically.

Input: `{ workspace, operations:[{ tool, args }...], atomic?, compact? }`

Semantics:

- If `atomic=true`, any failure rolls back all operations. Atomic mode requires undoable
  tools only (currently: `tasks_patch`, `tasks_task_define`, `tasks_progress`, `tasks_block`).
- Each operation is equivalent to a single tool call; `workspace` is injected if omitted
  in `args`.
- If `compact=true`, per-operation results are compacted (best-effort).

### `tasks_history`

Return operation history for undo/redo.

Input: `{ workspace, task?, limit? }`

### `tasks_undo` / `tasks_redo`

Apply the most recent undoable operation (or redo).

Semantics:

- Undo/redo are deterministic and version-gated.
- Not all operations are undoable; if none exist, the call errors.

## Evidence (v0.2)

### `tasks_evidence_capture`

Attach artifacts/checks to a step (or task/plan root).

Input: `{ workspace, task, path?, items?, checks?, attachments? }`

Semantics:

- Does not complete steps; it only records evidence.
- Artifacts are bounded by `max_items` and `max_artifact_bytes`.

## Context views (v0.2)

### `tasks_resume`

Load a plan/task with optional timeline events.

Input: `{ workspace, task?, plan?, events_limit?, read_only? }`

Semantics:

- If `read_only=true`, no focus/refs changes are performed.
- If `read_only=false` and `task`/`plan` differs from current focus, focus is restored to the target.
- When focus changes, response includes `focus_restored=true` and `focus_previous`.

### `tasks_resume_pack`

Unified resume: radar + timeline + decisions/evidence/blockers (bounded).

Input: `{ workspace, task?, plan?, events_limit?, decisions_limit?, evidence_limit?, max_chars?, read_only? }`

Semantics:

- If `read_only=true`, no focus/refs changes are performed.
- If `read_only=false` and `task`/`plan` differs from current focus, focus is restored to the target.
- When focus changes, response includes `focus_restored=true` and `focus_previous`.

### `tasks_resume_super`

Unified супер‑резюме: задачи + reasoning‑пакет + явные сигналы деградации/усечений.

Input: `{ workspace, task?, plan?, events_limit?, decisions_limit?, evidence_limit?, blockers_limit?, notes_limit?, trace_limit?, cards_limit?, max_chars?, read_only? }`

Semantics:

- Если `read_only=true`, никаких изменений фокуса/refs; refs вычисляются детерминированно.
- Возвращает `radar`, `steps`, `timeline`, `signals`, `memory` (notes/trace/cards), `degradation`.
- `degradation.truncated_fields` перечисляет поля, которые были усечены из‑за бюджета.

### `tasks_context_pack`

Bounded summary: radar + delta slice.

Input: `{ workspace, task?, plan?, max_chars?, delta_limit?, read_only? }`

### `tasks_mirror`

Export a compact plan slice for external consumers.

Input: `{ workspace, task?, plan?, path?, limit? }`

### `tasks_handoff`

Shift report: done/remaining/risks + radar core.

Input: `{ workspace, task?, plan?, limit?, max_chars?, read_only? }`

### `tasks_lint`

Read-only integrity checks for a plan/task.

Input: `{ workspace, task?, plan? }`

Semantics:

- Возвращает `context_health` с причинами деградации контекста и рекомендациями восстановления.

## DX‑макросы (v0.3)

### `tasks_macro_start`

One‑call запуск: создать задачу+шаги+гейты, вернуть супер‑резюме.

Input: `{ workspace, plan?, parent?, plan_title?, task_title, description?, steps, resume_max_chars? }`

Output: `{ task_id, plan_id?, steps, resume }`

### `tasks_macro_close_step`

One‑call закрытие шага: подтвердить чекпойнты → закрыть → вернуть супер‑резюме.

Input: `{ workspace, task, path?|step_id?, checkpoints, expected_revision?, resume_max_chars? }`

Output: `{ task, revision, step, resume }`

### `tasks_macro_finish`

One‑call завершение задачи: tasks_complete → handoff.

Input: `{ workspace, task, status?, handoff_max_chars? }`

Output: `{ task, status, handoff }`

## Subplans (v0.2)

### `tasks_task_add`

Add a task node inside a step plan.

Input: `{ workspace, task, parent_step, title, ... }`

### `tasks_task_define`

Update a task node inside a step plan.

Input: `{ workspace, task, path, ... }`

Notes: task node `path` uses `s:0.t:1` (parent step path + `.t:<ordinal>`).

### `tasks_task_delete`

Delete a task node inside a step plan.

Input: `{ workspace, task, path }`

## Templates & utilities (v0.2)

### `tasks_templates_list`

List built-in templates for scaffolding.

Output: `{ templates:[{ id, kind, title, description, plan_steps?, steps? }] }`

### `tasks_scaffold`

Create a plan/task from a template.

Input (union):

- Plan: `{ workspace, template, kind:"plan", title, description? }`
- Task: `{ workspace, template, kind:"task", title, parent?, plan_title?, description? }`

Semantics:

- `parent` is a `PLAN-###` id; `plan_title` creates a plan on the fly (mutually exclusive).
- Applies template checklist/steps with checkpoints as appropriate.

### `tasks_storage`

Return storage paths and namespaces.

Output: `{ storage_dir, defaults:{ branch, docs:{ notes, graph, trace } } }`

## Fast-path close (v0.1)

### `tasks_close_step`

Atomically confirms checkpoints and closes a step in a single call.

Input:

```json
{
  "workspace": "acme/repo",
  "task": "TASK-001",
  "step_id": "STEP-XXXXXXXX",
  "path": "s:0.s:1",
  "expected_revision": 4,
  "checkpoints": {
    "criteria": { "confirmed": true },
    "tests": { "confirmed": true },
    "security": { "confirmed": true },
    "perf": { "confirmed": true },
    "docs": { "confirmed": true }
  }
}
```

## DX macros (v0.3)

### `tasks_bootstrap`

One-call bootstrap: create plan (optional), task, steps, and checkpoints.

Input: `{ workspace, plan?, parent?, plan_title?, task_title, description?, steps[], think? }`
where each step requires `title`, `success_criteria[]`, `tests[]`, optional `blockers[]`.

Optional `think` payload:

- `{ frame?, hypothesis?, test?, evidence?, decision?, status?, note_decision?, note_title?, note_format? }`
- Seeds the task reasoning pipeline via `branchmind_think_pipeline` with strict defaults.

Semantics:

- Accepts either `plan` (existing) or `plan_title` (create new plan).
- Rejects empty `success_criteria` or `tests` to keep checkpoints gate-ready.
- When `think` is provided, output includes `think_pipeline` (or a warning if seeding fails).

Output:

```json
{
  "task": "TASK-001",
  "revision": 5,
  "step": { "step_id": "STEP-XXXXXXXX", "path": "s:0.s:1" },
  "events": [
    { "type": "step_verified", "...": "..." },
    { "type": "step_done", "...": "..." }
  ]
}
```

Semantics:

- Requires `checkpoints` (at least one of criteria/tests) and enforces gating.
- Emits both events in order (`step_verified` then `step_done`).
- Atomic: either both updates persist, or neither does.
