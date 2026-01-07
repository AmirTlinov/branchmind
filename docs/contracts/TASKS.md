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

### Step lease (optional, v0.5)

Step leases are an **optional** multi-agent safety mechanism (“room lock”):

- Lease scope is a **single step** (`step_id`), not a task.
- Leases are identified by `agent_id` and stored in the execution store.
- When a lease is active, **step mutations** must be performed by the lease holder, otherwise the server fails with `STEP_LEASE_HELD`.

Lease liveness is deterministic:

- A lease has an `expires_seq` (workspace event sequence).
- The lease is considered expired when `current_event_seq >= expires_seq`.
- Expired leases are treated as absent by read ops; write ops may garbage-collect expired rows.

#### `tasks_step_lease_get`

Inspect current lease state for a step.

Input: `{ workspace, task, path|step_id }`

Output: `{ step, lease?, now_seq }` where `lease` is absent when no active lease exists.

#### `tasks_step_lease_claim`

Claim a lease for the given step.

Input: `{ workspace, task, path|step_id, agent_id, ttl_seq?, force? }`

Semantics:

- When no active lease exists: claim succeeds and sets `expires_seq = now_seq + ttl_seq`.
- When the caller already holds the lease: claim is idempotent (no-op) unless `force=true`.
- When another agent holds the lease:
  - `force=false` fails with `STEP_LEASE_HELD`.
  - `force=true` takes over the lease and emits a takeover event (explicit, opt-in).

#### `tasks_step_lease_renew`

Extend an existing lease held by the caller.

Input: `{ workspace, task, path|step_id, agent_id, ttl_seq? }`

Semantics:

- Requires an active lease held by `agent_id`, otherwise fails with `STEP_LEASE_NOT_HELD`.
- Updates `expires_seq = now_seq + ttl_seq`.

#### `tasks_step_lease_release`

Release an existing lease held by the caller.

Input: `{ workspace, task, path|step_id, agent_id }`

Semantics:

- Requires an active lease held by `agent_id`, otherwise fails with `STEP_LEASE_NOT_HELD`.

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

### `tasks_context`

List plans and tasks in a workspace (paged).

Input: `{ workspace, plans_limit?, plans_cursor?, tasks_limit?, tasks_cursor?, max_chars? }`

Output (shape, abridged):

```json
{
  "workspace": "acme/repo",
  "counts": { "plans": 12, "tasks": 93 },
  "plans": [
    { "id": "PLAN-001", "kind": "plan", "title": "…", "status": "ACTIVE", "created_at_ms": 0, "updated_at_ms": 0 }
  ],
  "tasks": [
    { "id": "TASK-001", "kind": "task", "title": "…", "status": "TODO", "created_at_ms": 0, "updated_at_ms": 0 }
  ],
  "plans_pagination": { "cursor": 0, "next_cursor": 50, "count": 50, "limit": 50, "total": 120 },
  "tasks_pagination": { "cursor": 0, "next_cursor": 50, "count": 50, "limit": 50, "total": 930 }
}
```

Notes:

- `created_at_ms` / `updated_at_ms` are millisecond timestamps (unix epoch).
- When `max_chars` is provided, items may be compacted (optional fields dropped) and warnings are emitted.

### `tasks_resume`

Load a plan/task with optional timeline events.

Input: `{ workspace, task?, plan?, events_limit?, read_only? }`

Semantics:

- If `read_only=true`, no focus/refs changes are performed.
- If `read_only=false` and `task`/`plan` differs from current focus, focus is restored to the target.
- When focus changes, response includes `focus_restored=true` and `focus_previous`.
- When the target is a task, `steps[]` includes timestamps:
  - `created_at_ms` — step created time
  - `updated_at_ms` — last update time
  - `completed_at_ms` — completion time (null when not completed)

### `tasks_resume_pack`

Unified resume: radar + timeline + decisions/evidence/blockers (bounded).

Input: `{ workspace, task?, plan?, events_limit?, decisions_limit?, evidence_limit?, max_chars?, read_only? }`

Semantics:

- If `read_only=true`, no focus/refs changes are performed.
- If `read_only=false` and `task`/`plan` differs from current focus, focus is restored to the target.
- When focus changes, response includes `focus_restored=true` and `focus_previous`.

### `tasks_resume_super`

Unified супер‑резюме: задачи + reasoning‑пакет + явные сигналы деградации/усечений.

Input: `{ workspace, task?, plan?, view?, context_budget?, agent_id?, events_limit?, decisions_limit?, evidence_limit?, blockers_limit?, notes_limit?, trace_limit?, cards_limit?, notes_cursor?, trace_cursor?, cards_cursor?, graph_diff?, graph_diff_cursor?, graph_diff_limit?, engine_signals_limit?, engine_actions_limit?, max_chars?, read_only? }`

Semantics:

- Если `read_only=true`, никаких изменений фокуса/refs; refs вычисляются детерминированно.
- Возвращает `radar`, `steps`, `timeline`, `signals`, `memory` (notes/trace/cards), `degradation`, `capsule`.
- `memory.trace.sequential` may include a derived branching graph when trace entries are `trace_sequential_step`.
- May include an `engine` block (signals + actions) derived from the returned `memory.cards` + `memory.trace` slice (read-only, deterministic, slice-based).
- `context_budget` is a convenience knob for “smart context in N chars”:
  - Sets the effective output budget (`max_chars`) unless a smaller `max_chars` is explicitly provided.
  - If `view` is omitted, defaults to `view="smart"` (relevance-first).
- `view` controls relevance vs completeness:
  - `view="full"` (default): full super-resume envelope (bounded by limits + `max_chars`).
  - `view="smart"`: relevance-first envelope (current frontier + pinned + recent); archive is minimized by default.
    - May include `step_focus` (like `focus_only`) when a first open step exists.
    - When `step_focus` is present, `memory.trace.entries` is biased to the focused step (anti-noise): note entries via `meta.step`, events via `{ task_id, path }`.
  - `view="explore"`: relevance-first, but with a **warm archive** bias (more history is allowed in the “recent” slice).
    - Intended for research/design/architecture moments when you want connections and context, not only open items.
    - Still deterministic and bounded by section limits + `max_chars`/`context_budget`.
    - May include `step_focus` (like `smart`) when a first open step exists.
  - `view="audit"`: relevance-first, but with **all lanes visible** (shared + all `lane:agent:*`).
    - Intended for multi-agent sync/debug moments (explicit opt-in to avoid noise).
    - Still cold-archive by default (focus on open + pinned + step-scoped); use `explore`/`full` when you explicitly want history.
    - `agent_id` does not filter results in this view.
    - May include a small `lane_summary` derived from the returned slice (counts + top pinned/open per lane) to aid multi-agent coordination.
  - `view="focus_only"`: return **only the current step focus** + most relevant open context; archive is aggressively minimized.
    - Adds `step_focus` with `{ step, detail }` for the first open step when available.
      - `step_focus.detail` includes checkpoint/proof status fields (e.g. `*_confirmed`, `proof_*_mode`, `proof_*_present`) to support proof-first closing.
      - May include `step_focus.detail.lease` when step leases are enabled/used (holder + expiry metadata).
    - Filters `timeline.events` to the current step path (best-effort).
    - Reduces `memory` to a small relevant subset (open/pinned/engine-referenced), even when larger limits were requested.
    - Disables `graph_diff` by default (unless explicitly requested).
- `steps` includes compact gating/proof status:
  - `missing_proof_*` counts track **proof-required** steps that still lack attached proofs.
  - `first_open` may include `proof_*_mode` (`off|warn|require`) and `proof_*_present` flags.
- `degradation.truncated_fields` перечисляет поля, которые были усечены из‑за бюджета.
- `notes_cursor` / `trace_cursor` / `cards_cursor` продолжают пагинацию соответствующих разделов.
- `notes_limit=0` disables `memory.notes.entries` (returns an empty array).
- `trace_limit=0` disables `memory.trace.entries` (returns an empty array).
- `graph_diff=true` добавляет `graph_diff` (diff against base branch when available).
- `capsule` — версия‑стабильный “handoff‑контейнер” малого размера: цель/состояние, короткий handoff (done/remaining/risks), счётчики, последняя активность, и рекомендованное следующее действие (с подсказкой об эскалации toolset при необходимости).
  - May include a minimal **HUD where-block** (e.g. lane + step focus metadata) to make resumption copy/paste-safe and orientation-free.
  - When step leases are used, the HUD may include lease metadata under `capsule.where.step_focus.lease`.
  - Under very tight `max_chars` budgets, the server may degrade the payload to a **capsule-only** result (still typed + deterministic) rather than returning an empty/minimal signal.
- `capsule.action` может учитывать гейтинг чекпойнтов (включая optional категории `security/perf/docs`, которые становятся required, если к ним привязано evidence).
- В `view="smart"`/`view="focus_only"` для TASK может добавляться `capsule.prep_action` (two-step flow): обычно это `think_pipeline` с `step="focus"` как подготовка reasoning перед прогресс-операцией (`capsule.action`).
- `agent_id` (optional) may influence how reasoning memory is filtered in relevance-first views (smart/focus_only/explore): shared anchors plus the agent lane are preferred; legacy cards/notes without a lane stamp are treated as shared.

### `tasks_snapshot`

Unified snapshot: задачи + reasoning + diff.

Input: `{ workspace, task?, plan?, view?, context_budget?, agent_id?, events_limit?, decisions_limit?, evidence_limit?, blockers_limit?, notes_limit?, trace_limit?, cards_limit?, notes_cursor?, trace_cursor?, cards_cursor?, graph_diff_cursor?, graph_diff_limit?, engine_signals_limit?, engine_actions_limit?, max_chars?, read_only? }`

Semantics:

- Wrapper around `tasks_resume_super` with a more explicit portal intent.
- Default view: when `view` is omitted, the wrapper sets `view="smart"` (relevance-first, cold archive) to keep the portal fast and low-noise.
- When `view="focus_only"` or `view="smart"` or `view="explore"` or `view="audit"`, `graph_diff` is not auto-enabled by the wrapper (relevance-first views prefer budget).
- In legacy usage (no relevance-first view + no `context_budget`), the wrapper auto-enables `graph_diff=true` to keep snapshots useful without extra calls.
- `graph_diff` включает summary и пагинацию diff-изменений.
- Наследует `capsule` из `tasks_resume_super`; это рекомендуемый “one‑screen handoff” для подхвата задач другим агентом.
- Output note (BM‑L1 portals): portal tools render a compact line protocol by default (state + next action).
  Use `tasks_resume_super` when you need the structured JSON envelope payload.

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

Input: `{ workspace, agent_id?, plan?, parent?, plan_title?, plan_template?, task_title, description?, template?, steps?, think?, view?, resume_max_chars? }`

Output: `{ task_id, plan_id?, steps, resume }`

Semantics:

- Provide either `steps` or `template` (not both). If neither is provided, the macro defaults to `template="basic-task"` to keep daily usage low-syntax while preserving step discipline.
- `template` uses built‑in task templates (see `tasks_templates_list`) to reduce input verbosity.
- `plan_template` is allowed only when creating a new plan via `plan_title`.
- If `plan`/`parent` and `plan_title` are both provided, `plan_title` acts as a consistency check: it must match the referenced plan’s stored title, otherwise the call fails with `INVALID_INPUT` (prevents silent mis-targeting).
- If neither `plan` nor `parent` nor `plan_title` is provided and the workspace focus is a plan, that plan is used as the implicit parent (focus-first DX).
- If neither `plan` nor `parent` nor `plan_title` is provided and the workspace focus is a task, the macro uses that task’s parent plan (stay-in-plan DX).
- If no implicit parent can be derived from focus, the macro uses the first plan with title `"Inbox"` if it exists; otherwise it creates a new `"Inbox"` plan.
- If `think` is provided, it is forwarded to the bootstrap pipeline and the response may include `think_pipeline` as confirmation.
- Reasoning-first default for principal tasks: if `template="principal-task"` and `think` is omitted, the server seeds a minimal `think.frame` card derived from `task_title` and `description` (deterministic, low-noise).
- The returned `resume` is a `tasks_resume_super` snapshot:
  - If `view` is omitted, defaults to `view="smart"` (relevance-first, cold archive).
  - `view` is forwarded to the underlying resume call, so callers can opt into `explore`/`audit` explicitly.
  - If `agent_id` is present, it is forwarded to the resume call (lane filtering + multi-agent lease-aware actions).

### `tasks_macro_close_step`

One‑call закрытие шага: подтвердить чекпойнты → закрыть → вернуть супер‑резюме.

Input: `{ workspace, agent_id?, task?, path?|step_id?, checkpoints?, expected_revision?, note?, proof?, view?, resume_max_chars? }`

Output: `{ task, revision, step, resume }`

Semantics:

- If `note` is provided, the server records a progress note before closing the step (and returns `note_event`).
- If `proof` is provided, the server captures evidence before closing the step (proof‑first) and returns `evidence_event`.
  - `proof` is an agent‑DX shortcut for `tasks_evidence_capture` scoped to the same step:
    - string → treated as `checks[]` (untagged lines are auto‑normalized to receipts: bare URL → `LINK: ...`, otherwise → `CMD: ...`),
    - array of strings → treated as `checks[]` (same auto‑normalization as the string form),
    - object → forwarded as `{ items?, checks?, attachments?, checkpoint? }` (same shape as `tasks_evidence_capture` minus target fields).
  - If `checkpoint` is omitted, proof is linked to `"tests"` by default.
  - Input DX: `checks[]` lines may be pasted as markdown (bullets like `- ...`, `* ...`, `1. ...`); list prefixes are ignored.
- `checkpoints` defaults to `"gate"` when omitted.
- `checkpoints` can be either:
  - `"gate"` (string shortcut: criteria+tests), or
  - `"all"` (string shortcut), or
  - object form (v0.2), including shorthand booleans like `{ "criteria": true }`.
- If neither `path` nor `step_id` is provided, the server closes the **first open step** of the focused task (focus-first DX).
- If the task has **no open steps**, the macro is treated as “advance progress” and will attempt to finish the task
  deterministically (set status to `DONE`) and return an updated super-resume (idempotent if already `DONE`).
- The returned `resume` is a `tasks_resume_super` snapshot:
  - If `view` is omitted, defaults to `view="smart"` (relevance-first, cold archive).
  - `view` is forwarded to the underlying resume call; use `audit` when you explicitly want cross-lane visibility.
  - If `agent_id` is present, it is forwarded to the resume call (lane filtering + multi-agent lease-aware actions).
- Proof requirements are **hybrid** (warning-first + require-first by policy):
  - By default, steps do not require proofs.
  - Built-in “principal” templates may mark specific steps (e.g. “Verify with proofs”) as proof-required for `tests`.
  - When a step requires proof for a checkpoint and proof is missing, closing fails with `error.code="PROOF_REQUIRED"` and a portal-first recovery suggestion.
  - Soft proof lint: proof checks are treated as receipts (`CMD:` + `LINK:`). If one of the receipts is missing or still a placeholder, the server emits `WARNING: PROOF_WEAK` (does not block closing).
    - A URL-like `attachments[]` entry counts as a `LINK` receipt for this soft lint (avoids false warnings when the link is attached rather than typed as `LINK:`).
  - Placeholders do not count as proof: any `<fill: ...>` receipts are ignored and must be replaced with real commands/links to satisfy proof-required steps.

### `tasks_macro_finish`

One‑call завершение задачи: tasks_complete → handoff.

Input: `{ workspace, task, status?, final_note?, handoff_max_chars? }`

Output: `{ task, status, complete?, final_note?, handoff }`

Notes:

- Idempotent: if the task is already in the requested `status`, the macro does not emit a new completion event.
- If `final_note` is provided, it is appended to the task reasoning `notes_doc` (artifact) before generating the handoff.
- If `status="DONE"` but there are open steps, the macro fails fast with portal-first recovery suggestions
  (close remaining steps via `tasks_macro_close_step`, then retry).
- This macro is not part of the daily portal toolset by default (surface budget).
  Daily flow finishes tasks via `tasks_macro_close_step` when no open steps remain.

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

Input: `{ workspace, agent_id?, plan?, parent?, plan_title?, plan_template?, task_title, description?, template?, steps[]?, think? }`
where each step requires `title`, `success_criteria[]`, `tests[]`, optional `blockers[]`.

Optional `think` payload:

- `{ agent_id?, frame?, hypothesis?, test?, evidence?, decision?, status?, note_decision?, note_title?, note_format? }`
- Seeds the task reasoning pipeline via `think_pipeline` with strict defaults (no branch/doc overrides).

Semantics:

- Accepts either `plan` (existing) or `plan_title` (create new plan).
- Provide either `steps` or `template` (not both).
- `template` uses built‑in task templates (see `tasks_templates_list`) to reduce input verbosity.
- `plan_template` is allowed only when creating a new plan via `plan_title` (applies a checklist).
- Rejects empty `success_criteria` or `tests` to keep checkpoints gate-ready.
- When `think` is provided, output includes `think_pipeline` (or a warning if seeding fails).
- If top-level `agent_id` is provided and `think.agent_id` is omitted, the server forwards `agent_id` to `think_pipeline` (lane consistency in multi-agent usage).

Output:

```json
{
  "workspace": "acme/repo",
  "plan": { "id": "PLAN-001", "qualified_id": "acme/repo:PLAN-001", "created": false },
  "task": { "id": "TASK-001", "qualified_id": "acme/repo:TASK-001", "revision": 5 },
  "steps": [
    { "step_id": "STEP-XXXXXXXX", "path": "s:0" }
  ],
  "events": [],
  "think_pipeline": { "created": [], "decision_note": null }
}
```
