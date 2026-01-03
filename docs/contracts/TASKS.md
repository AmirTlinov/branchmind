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

## Unified targeting (v0.3)

All task tools accept a `target` selector as an alias for `task`/`plan`.

Examples:

- `{ workspace, target: "TASK-001" }`
- `{ workspace, target: { "id": "PLAN-001", "kind": "plan" } }`

### Focus-first targeting (DX rule)

To keep daily agent usage **cognitively cheap**, any tool that operates on a specific plan/task may omit
`task`/`plan`/`target` **if the workspace focus is set**.

- Explicit `task`/`plan`/`target` always wins.
- If no explicit target is provided, the tool uses the current workspace focus (set via `tasks_focus_set`).
- Tools that are workspace-scoped (e.g., `tasks_context`, `tasks_delta`) ignore focus.
- Focus is convenience only: tools must not silently change focus as a side-effect.

## Payload budgets (v0)

- `tasks_context`, `tasks_delta`, `tasks_radar`, `tasks_handoff`, `tasks_context_pack`, `tasks_resume_pack`, `tasks_resume_super` accept optional `max_chars`.
- When `max_chars` is provided, responses are deterministically truncated and emit warnings (`BUDGET_TRUNCATED`, `BUDGET_MINIMAL`, `BUDGET_MIN_CLAMPED`).
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
- Context views: `tasks_resume`, `tasks_resume_pack`, `tasks_resume_super`, `tasks_snapshot`, `tasks_context_pack`, `tasks_mirror`, `tasks_handoff`, `tasks_lint`
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
- Responses include `qualified_id` (`<workspace>:<id>`) for stable cross-workspace referencing.

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

Input: `{ workspace, task, path?, step_id?, items?, checks?, attachments?, checkpoint? }`

Semantics:

- Does not complete steps; it only records evidence.
- Artifacts are bounded by `max_items` and `max_artifact_bytes`.
- `checkpoint` is optional and may be either:
  - a string (`"security"`), or
  - an array of strings (`["security","docs"]`).
- Allowed checkpoint kinds: `criteria`, `tests`, `security`, `perf`, `docs` (invalid values are rejected).
- If `checkpoint` is provided, the captured evidence is linked to that checkpoint (or checkpoints) for the target entity.
  - This makes optional checkpoints (security/perf/docs) become **required** for step completion once evidence exists.

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

Input: `{ workspace, task?, plan?, events_limit?, decisions_limit?, evidence_limit?, blockers_limit?, notes_limit?, trace_limit?, cards_limit?, notes_cursor?, trace_cursor?, cards_cursor?, graph_diff?, graph_diff_cursor?, graph_diff_limit?, max_chars?, read_only? }`

Semantics:

- Если `read_only=true`, никаких изменений фокуса/refs; refs вычисляются детерминированно.
- Возвращает `radar`, `steps`, `timeline`, `signals`, `memory` (notes/trace/cards), `degradation`, `capsule`.
- `steps` includes compact gating/proof status:
  - `missing_proof_*` counts track **proof-required** steps that still lack attached proofs.
  - `first_open` may include `proof_*_mode` (`off|warn|require`) and `proof_*_present` flags.
- `degradation.truncated_fields` перечисляет поля, которые были усечены из‑за бюджета.
- `notes_cursor` / `trace_cursor` / `cards_cursor` продолжают пагинацию соответствующих разделов.
- `graph_diff=true` добавляет `graph_diff` (diff against base branch when available).
- `capsule` — версия‑стабильный “handoff‑контейнер” малого размера: цель/состояние, короткий handoff (done/remaining/risks), счётчики, последняя активность, и рекомендованное следующее действие (с подсказкой об эскалации toolset при необходимости).
- `capsule.action` может учитывать гейтинг чекпойнтов (включая optional категории `security/perf/docs`, которые становятся required, если к ним привязано evidence).

### `tasks_snapshot`

Unified snapshot: задачи + reasoning + diff.

Input: `{ workspace, task?, plan?, events_limit?, decisions_limit?, evidence_limit?, blockers_limit?, notes_limit?, trace_limit?, cards_limit?, notes_cursor?, trace_cursor?, cards_cursor?, graph_diff_cursor?, graph_diff_limit?, max_chars?, read_only? }`

Semantics:

- Эквивалентно `tasks_resume_super` с `graph_diff=true` и более явным intent.
- `graph_diff` включает summary и пагинацию diff-изменений.
- Наследует `capsule` из `tasks_resume_super`; это рекомендуемый “one‑screen handoff” для подхвата задач другим агентом.

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

DX note (default workspace):

- Portal macros may accept omitted `workspace` when the server is configured with a default workspace (`--workspace` / `BRANCHMIND_WORKSPACE`).
- Explicit `workspace` always wins.

### `tasks_macro_start`

One‑call запуск: создать задачу+шаги+гейты, вернуть супер‑резюме.

Input: `{ workspace, plan?, parent?, plan_title?, plan_template?, task_title, description?, template?, steps?, think?, resume_max_chars? }`

Output: `{ task_id, plan_id?, steps, resume }`

Semantics:

- Provide either `steps` or `template` (not both). If neither is provided, the macro defaults to `template="basic-task"` to keep daily usage low-syntax while preserving step discipline.
- `template` uses built‑in task templates (see `tasks_templates_list`) to reduce input verbosity.
- `plan_template` is allowed only when creating a new plan via `plan_title`.
- If neither `plan` nor `parent` nor `plan_title` is provided and the workspace focus is a plan, that plan is used as the implicit parent (focus-first DX).
- If neither `plan` nor `parent` nor `plan_title` is provided and the workspace focus is a task, the macro uses that task’s parent plan (stay-in-plan DX).
- If no implicit parent can be derived from focus, the macro uses the first plan with title `"Inbox"` if it exists; otherwise it creates a new `"Inbox"` plan.
- If `think` is provided, it is forwarded to the bootstrap pipeline and the response may include `think_pipeline` as confirmation.
- Reasoning-first default for principal tasks: if `template="principal-task"` and `think` is omitted, the server seeds a minimal `think.frame` card derived from `task_title` and `description` (deterministic, low-noise).

### `tasks_macro_close_step`

One‑call закрытие шага: подтвердить чекпойнты → закрыть → вернуть супер‑резюме.

Input: `{ workspace, task?, path?|step_id?, checkpoints?, expected_revision?, note?, proof?, resume_max_chars? }`

Output: `{ task, revision, step, resume }`

Semantics:

- If `note` is provided, the server records a progress note before closing the step (and returns `note_event`).
- If `proof` is provided, the server captures evidence before closing the step (proof‑first) and returns `evidence_event`.
  - `proof` is an agent‑DX shortcut for `tasks_evidence_capture` scoped to the same step:
    - string → treated as `checks[]` (untagged lines are auto‑normalized to receipts: bare URL → `LINK: ...`, otherwise → `CMD: ...`),
    - array of strings → treated as `checks[]` (same auto‑normalization as the string form),
    - object → forwarded as `{ items?, checks?, attachments?, checkpoint? }` (same shape as `tasks_evidence_capture` minus target fields).
  - If `checkpoint` is omitted, proof is linked to `"tests"` by default.
- `checkpoints` defaults to `"gate"` when omitted.
- `checkpoints` can be either:
  - `"gate"` (string shortcut: criteria+tests), or
  - `"all"` (string shortcut), or
  - object form (v0.2), including shorthand booleans like `{ "criteria": true }`.
- If neither `path` nor `step_id` is provided, the server closes the **first open step** of the focused task (focus-first DX).
- If the task has **no open steps**, the macro is treated as “advance progress” and will attempt to finish the task
  deterministically (set status to `DONE`) and return an updated super-resume (idempotent if already `DONE`).
- Proof requirements are **hybrid** (warning-first + require-first by policy):
  - By default, steps do not require proofs.
  - Built-in “principal” templates may mark specific steps (e.g. “Verify with proofs”) as proof-required for `tests`.
  - When a step requires proof for a checkpoint and proof is missing, closing fails with `error.code="PROOF_REQUIRED"` and a portal-first recovery suggestion.
  - Soft proof lint: proof checks are treated as receipts (`CMD:` + `LINK:`). If one of the receipts is missing or still a placeholder, the server emits `WARNING: PROOF_WEAK` (does not block closing).
  - Placeholders do not count as proof: any `<fill: ...>` receipts are ignored and must be replaced with real commands/links to satisfy proof-required steps.

### `tasks_macro_finish`

One‑call завершение задачи: tasks_complete → handoff.

Input: `{ workspace, task, status?, handoff_max_chars? }`

Output: `{ task, status, handoff }`

### `tasks_macro_create_done`

One‑call: создать задачу → закрыть первый шаг → завершить задачу.

Input: `{ workspace, plan?, parent?, plan_title?, task_title, description?, steps }`

Output: `{ bootstrap, close, complete }`

Semantics:

- `steps` must contain exactly one step (macro closes the first step immediately).

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

Shorthand:

- `checkpoints: "gate"` — compact shortcut for confirming only the gate checkpoints (criteria + tests).
- `checkpoints: "all"` — compact shortcut for confirming all checkpoint categories.
- Boolean shortcut is allowed inside object form, e.g. `{ "criteria": true, "tests": true }`.

## DX macros (v0.3)

### `tasks_bootstrap`

One-call bootstrap: create plan (optional), task, steps, and checkpoints.

Input: `{ workspace, plan?, parent?, plan_title?, plan_template?, task_title, description?, template?, steps[]?, think? }`
where each step requires `title`, `success_criteria[]`, `tests[]`, optional `blockers[]`.

Optional `think` payload:

- `{ frame?, hypothesis?, test?, evidence?, decision?, status?, note_decision?, note_title?, note_format? }`
- Seeds the task reasoning pipeline via `think_pipeline` with strict defaults.

Semantics:

- Accepts either `plan` (existing) or `plan_title` (create new plan).
- Provide either `steps` or `template` (not both).
- `template` uses built‑in task templates (see `tasks_templates_list`) to reduce input verbosity.
- `plan_template` is allowed only when creating a new plan via `plan_title` (applies a checklist).
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
