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

## Reasoning modes (v0.6)

Some task flows can optionally switch into a stricter reasoning discipline.

- `reasoning_mode` is a task-level setting (string enum): `normal` | `deep` | `strict`.
- `normal` (default): reasoning engine output is advisory; task/step lifecycle is driven by checkpoints only.
- `deep`: same as `normal` for enforcement, but intended for “principal” work where agents are encouraged to keep
  hypotheses/tests/evidence explicit and versioned.
- `strict`: **enforcement mode**. Certain task lifecycle operations (notably “close step”) are blocked when the
  reasoning engine detects missing discipline signals (e.g. no test for an open hypothesis, or supports without a
  counter-position).
  - Status note: for strict gating, hypothesis/decision cards are treated as “active” unless explicitly closed
    (`status=closed|done|resolved`). This prevents accidental bypass via status drift (e.g. “accepted”).

Determinism rules (MUST):

- Strict gating must depend only on persisted artifacts (graph + trace + receipts); no wall clock and no network.
- Recovery must be portal-first: return at least one concrete next action suggestion (typically `think_card`) with
  step-scoped params so the agent can fix the issue without hunting.

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
- Context views: `tasks_resume`, `tasks_resume_pack`, `tasks_resume_super`, `tasks_snapshot`, `tasks_context_pack`, `tasks_mindpack`, `tasks_mirror`, `tasks_handoff`, `tasks_lint`
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

Input: `{ workspace, task, expected_revision?, status?, parked_for_s?, parked_until_ts_ms? }` where `status` is `TODO|ACTIVE|DONE|PARKED|CANCELED` (default `DONE`).

Semantics:

- Uses optimistic concurrency (`expected_revision`).
- For tasks, completion requires all steps to be completed and checkpoints confirmed.
- `PARKED` is a non-terminal “out of active horizon” status (keeps history, reduces noise).
- `CANCELED` is a terminal “intentionally stopped” status (also out of horizon by default).
- Time-based snooze (tasks only): when `status="PARKED"`, the caller may provide **exactly one** of:
  - `parked_for_s` (relative; server computes `parked_until_ts_ms = now_ms + parked_for_s*1000`), or
  - `parked_until_ts_ms` (absolute unix ms).
  If neither is provided, the task is parked indefinitely (manual wake).
  If `status!="PARKED"`, providing `parked_for_s`/`parked_until_ts_ms` is an `INVALID_INPUT` error.

### `tasks_edit`

Edit plan/task metadata.

Input (union):

- Plan: `{ workspace, task, title?, description?, context?, priority?, tags?, depends_on?, contract?, contract_data? }`
- Task: `{ workspace, task, title?, description?, context?, priority?, new_domain?, reasoning_mode?, tags?, depends_on? }`

Semantics:

- Applies an atomic patch and emits a single event.
- `new_domain` maps to task `domain`.
- `reasoning_mode` is stored on the task and affects strict gating on step closure (see “Reasoning modes”).

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
- Step patch fields (v0.10):
  - lists: `success_criteria[]`, `tests[]`, `blockers[]` (via `append|remove|set|unset`)
  - scalars: `next_action`, `stop_criteria` (via `set|unset`)
  - proof modes: `proof_tests_mode|proof_security_mode|proof_perf_mode|proof_docs_mode` (via `set|unset`, values `off|warn|require`)

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
  - In reduced toolsets (`core`/`daily`), engine actions may include a `call_method` → `tools/list` disclosure step
    to reveal the minimal toolset tier required before executing `call_tool` actions. This keeps engine actions
    executable for clients that enforce “advertised tools only”.
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
  - `view="audit"`: relevance-first, but with **drafts visible** (explicit opt-in to avoid noise).
    - Includes artifacts tagged `v:draft` and legacy `lane:agent:*` artifacts.
    - Still cold-archive by default (focus on open + pinned + step-scoped); use `explore`/`full` when you explicitly want history.
    - `agent_id` does not filter results in this view.
    - May include a small `lane_summary` derived from the returned slice (legacy lanes only; best-effort).
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
  - May include a meaning-map HUD under `capsule.where.map`:
    - `{ where:"a:..."|"unknown", top_anchors:["a:...", ...] }`
    - Derived deterministically from anchor tags (`a:*`) observed in the returned card slice.
    - When `step_focus` is present, `where` is computed from step-scoped cards (most recent + most frequent).
  - May include a horizon summary under `capsule.where.horizon` (plan focus):
    - `{ active, backlog, parked, stale, done, total, next_wake? }`
    - Derived deterministically from child task statuses and horizon metadata under the focused plan.
      - `active`: `status="ACTIVE"`.
      - `done`: `status="DONE"`.
      - `parked`: `status="PARKED"` where wake is not due (`parked_until_ts_ms` is null or `> now_ms`).
      - `backlog`: `status="TODO"` plus parked items whose wake time is due (`parked_until_ts_ms <= now_ms`).
      - `stale`: count of open items that have not changed recently (computed from `updated_at_ms` with a stale-after policy).
      - `total`: total child tasks under the plan (any status).
      - `next_wake`: optional single item, the earliest upcoming parked wake (bounded to 1).
    - BM‑L1 state line may print it as `horizon active=... backlog=... parked=... stale=... done=... total=...` to keep the default view navigable (counts, not lists).
  - May include `capsule.map_action` (copy/paste-safe meaning-map lens):
    - When `where="unknown"`, the portal may suggest attaching/registering an anchor to the current step (preferably via `macro_anchor_note step="focus" visibility=canon`).
      - The suggested attach note is canonical by default (`v:canon`) so the follow-up `open id="a:..." max_chars=8000` lens is not empty.
    - When `where="a:..."` is known, the portal may suggest `open id="a:..." max_chars=8000` to instantly load anchor-scoped context without scanning code.
  - May include `capsule.refs` (navigation safety net under budgets):
    - A small, bounded list of openable ids (e.g. `CARD-*` or `<doc>@<seq>`) derived from the returned slice.
    - Intended for BM-L1 `REFERENCE:` lines when output is budget-truncated.
  - Under very tight `max_chars` budgets, the server may degrade the payload to a **capsule-only** result (still typed + deterministic) rather than returning an empty/minimal signal.
- `capsule.action` может учитывать гейтинг чекпойнтов (включая optional категории `security/perf/docs`, которые становятся required, если к ним привязано evidence).
- В `view="smart"`/`view="focus_only"` для TASK может добавляться `capsule.prep_action` (pre-flight перед прогресс-операцией `capsule.action`):
  - В toolset=`daily`, когда `capsule.where.map.where="a:..."` уже известен, это может быть 1‑командный `think_card step="focus"` с тегами `[a:..., v:draft, skeptic:preflight]` — чтобы зафиксировать контр‑гипотезу/фальсификатор/критерий остановки **в привязке к anchor**, не создавая шума в дефолтных линзах.
  - Если `where="unknown"`, портал может предложить детерминированный `think_playbook name="skeptic"|\"strict\"` как безопасный fallback.
  - В toolset=`full`, обычно предлагается структурированный `think_pipeline step="focus"` (frame→hypothesis→test→evidence→decision).
- `agent_id` is optional and must not be required for durable resumption.
  - It is used for step leases (execution safety) and may be recorded as audit metadata.
  - It must not be used as a memory retrieval key in relevance-first views.

### `tasks_snapshot`

Unified snapshot: задачи + reasoning + diff.

Input: `{ workspace, task?, plan?, view?, context_budget?, agent_id?, delta?, refs?, delta_limit?, events_limit?, decisions_limit?, evidence_limit?, blockers_limit?, notes_limit?, trace_limit?, cards_limit?, notes_cursor?, trace_cursor?, cards_cursor?, graph_diff_cursor?, graph_diff_limit?, engine_signals_limit?, engine_actions_limit?, max_chars?, read_only? }`

Semantics:

- Wrapper around `tasks_resume_super` with a more explicit portal intent.
- Default view: when `view` is omitted, the wrapper sets `view="smart"` (relevance-first, cold archive) to keep the portal fast and low-noise.
- DX default: when `delta` is omitted, the wrapper sets `delta=true` in DX mode (`--dx` / `BRANCHMIND_DX=1` / zero‑arg auto).
- **Snapshot Navigation Guarantee v2 (BM‑L1):** the first (state) line always includes a stable navigation handle as `ref=<id>`.
  - Preferred: `ref=CARD-...` (pinned cockpit if present; otherwise the most recent `CARD-*` in the returned slice).
  - Fallback: `ref=TASK-...` / `ref=PLAN-...`.
  - This guarantee must hold even when the snapshot is budget-truncated (`BUDGET_TRUNCATED` / `BUDGET_MINIMAL`).
  - Deterministic selection (high level):
    - If a pinned cockpit card is present, prefer that `CARD-*`.
    - Else, if any `CARD-*` is present in the snapshot payload (or capsule refs), prefer the most recent deterministically.
    - Else fall back to the focused `TASK-*` / `PLAN-*` id.
- `REFERENCE:` lines are reserved for explicit navigation modes and must not appear by default.
  - When `refs=true`, the BM-L1 output may include an additional small, bounded set of `REFERENCE:` lines (openable ids like `CARD-*` or `<doc>@<seq>`) even when not truncated.
  - When additional `REFERENCE:` lines are emitted (delta mode, `refs=true`), the portal may also include a single bounded `open id=... max_chars=8000` jump command.
- When `view="focus_only"` or `view="smart"` or `view="explore"` or `view="audit"`, `graph_diff` is not auto-enabled by the wrapper (relevance-first views prefer budget).
- In legacy usage (no relevance-first view + no `context_budget`), the wrapper auto-enables `graph_diff=true` to keep snapshots useful without extra calls.
- `graph_diff` включает summary и пагинацию diff-изменений.
- Наследует `capsule` из `tasks_resume_super`; это рекомендуемый “one‑screen handoff” для подхвата задач другим агентом.
- `delta=true` включает режим “что изменилось с прошлого snapshot” для reasoning‑части:
  - Возвращает `delta` в JSON‑результате (см. ниже) и в BM‑L1 (`fmt=lines`) может добавить несколько `REFERENCE:` строк с новыми элементами.
  - Дельта собирается по reasoning scope (branch+docs) текущего target (plan/task) и **не требует** “угадывания” курсоров.
  - База дельты хранится per `(workspace, target)` и не должна зависеть от `agent_id` (чтобы рестарты не ломали дельту).
  - По умолчанию (когда базы ещё нет) сервер устанавливает базу “на сейчас” и возвращает пустую дельту (чтобы следующий вызов был осмысленным).
  - `delta_limit` ограничивает количество элементов в каждом из разделов (notes/cards/decisions/evidence); если лимит сработал, сервер возвращает счётчик `dropped` и помечает `truncated=true`.
- Output note (BM‑L1 portals): portal tools render a compact line protocol by default (state + next action).
  Use `tasks_resume_super` when you need the structured JSON envelope payload.
  - Second-brain DX: the BM‑L1 state line may include a stable mindpack pointer as `pack=mindpack@<seq>` so a new
    session can jump to the latest semantic compaction without hunting.

Delta payload (when `delta=true`):

- `delta`: `{ mode, since_seq, until_seq, notes, cards, decisions, evidence }`
  - `mode`: `"since_last"` (portal stored baseline).
  - `since_seq` / `until_seq`: doc-entry seq window used to compute the delta (monotonic per workspace).
  - Each section: `{ count, items, truncated, dropped }`
    - `notes.items[]`: `{ ref, seq, ts, title?, summary }` where `ref` is like `notes@123`.
    - `cards/decisions/evidence.items[]`: `{ id, type, seq, title?, summary }` where `id` is `CARD-...`.
    - `summary` is a deterministic 1-line meaning hint (title or first-line excerpt, trimmed).

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
- Возвращает `issues[]` (что не так) и **refactorable** `patches[]` (малые правки) + `actions[]`
  (copy/paste вызовы), чтобы “плохой план” не требовал переписывания.
- Делает **meaning hygiene** проверки (anchors):
  - для `task`: если у задачи нет anchor‑связей, добавляет `MISSING_ANCHOR` и предлагает one‑shot патч
    через `macro_anchor_note` (создать/привязать anchor к задаче),
  - для `plan`: если у плана есть `ACTIVE` задачи без anchor‑связей, добавляет `ACTIVE_TASKS_MISSING_ANCHOR`
    и предлагает ограниченный набор one‑shot патчей (по нескольким задачам), чтобы быстро закрыть KPI.
- Для `plan`-таргета включает anti‑kasha проверки Active Horizon:
  - если `ACTIVE` задач больше 3, добавляет предупреждение `ACTIVE_LIMIT_EXCEEDED`,
  - предлагает безопасные патчи “припарковать” часть задач обратно в `TODO` (через `tasks_complete`),
  - добавляет action для просмотра списка `ACTIVE` задач (чтобы выбрать, что парковать).
- Для `task`-таргета включает planning-quality проверки (refactorable, one-shot):
  - `MISSING_NEXT_ACTION` (`kind=actionless`): у первого открытого шага нет `next_action`.
  - `MISSING_PROOF_PLAN` (`kind=unproveable`): в задаче нет открытого шага с `proof_tests_mode=require` (и поэтому `DONE` легко станет “без пруфов”).
  - `RESEARCH_MISSING_STOP_CRITERIA` (`kind=unbounded`): у research‑шага нет `stop_criteria`.
  - Для каждого issue возвращается минимум один bounded patch через `tasks_patch` (copy/paste), чтобы агент мог улучшать план инкрементально.

Output shape (conceptual):

- `summary`: `{ errors, warnings, total }`
- `issues[]`: each issue includes:
  - `severity`: `error|warning`
  - `kind`: one of `unverifiable|unproveable|unbounded|unnavigable|self_contradicting|actionless|context`
  - `message`
  - `recovery` (short, actionable)
  - optional `target`: `{ kind: "task"|"plan"|"step", id?, step_id?, path? }`
- `patches[]`: each patch is a small suggested edit:
  - `id` (stable string)
  - `purpose` (short)
  - `apply`: `{ tool: "<tool_name>", arguments: {...} }` (copy/paste safe)
  - optional `notes`: guidance on what to fill in (no long prose)
- `actions[]`: additional copy/paste calls that do not change state (e.g. “open step detail”).

Determinism:

- Issue ordering is deterministic (by `severity`, then `kind`, then `target`).
- Patch ids are stable strings; patches are ordered by id.
- Lint never applies patches; it only suggests bounded changes.

### `tasks_mindpack` (v0.9)

Workspace mindpack: a bounded, versioned **semantic compaction** artifact for “resume by meaning”.

Input: `{ workspace, update?, reason?, max_chars?, read_only? }`

Semantics:

- `tasks_mindpack` returns the latest mindpack entry for the workspace as an openable doc ref:
  - `ref = mindpack@<seq>`
- Storage form:
  - appended as a **doc entry** under the workspace checkout branch (`doc="mindpack"`), so it is:
    - versioned (each entry has `seq`),
    - openable via `open id="mindpack@<seq>"`,
    - diff-friendly (stable ordering + bounded content).
- Update mode:
  - `update=true` computes the current mindpack from persisted state and appends a new entry **only if content changed**
    (dedupe; no spam).
  - `reason` is an optional short tag stored in mindpack meta (e.g. `"focus_change"`, `"close_step"`, `"job_done"`).
  - `read_only=true` computes and returns the would-be mindpack without writing (DX + tests).
- Mindpack content is a navigation index, not prose:
  - focus target (`TASK-*`/`PLAN-*`) + one best `ref`
  - plan horizon counts (active/backlog/done/other)
  - top anchors (≤3) + anchor coverage KPI (e.g. “active_missing_anchor”)
  - one primary next action + backup (when deterministically derivable)
  - `changed` block: ≤5 short lines, derived from prior mindpack meta (no chat hunting)
- Truncation invariants:
  - The first mindpack line is bounded and copy/paste-safe under tight budgets.
  - `tasks_snapshot` should surface `pack=mindpack@<seq>` near the top so /compact never loses the navigation handle.

Output shape (conceptual):

- `mindpack`: `{ ref, doc, seq, ts_ms, summary, meta }`
- `updated`: boolean (true when a new entry was appended)
- `changed[]`: bounded short change lines (may be empty)

## DX‑макросы (v0.3)

DX note (default workspace):

- When the server is configured with a default workspace (`--workspace` / `BRANCHMIND_WORKSPACE`), callers may omit `workspace` and it will default to that workspace.
- Explicit `workspace` always wins.

### `tasks_macro_start`

One‑call запуск: создать задачу+шаги+гейты, вернуть супер‑резюме.

Input: `{ workspace, agent_id?, plan?, parent?, plan_title?, plan_template?, task_title, description?, template?, steps?, think?, reasoning_mode?, view?, resume_max_chars? }`

Output: `{ task_id, plan_id?, steps, resume }`

Semantics:

- Provide either `steps` or `template` (not both). If neither is provided, the macro defaults to `template="basic-task"` to keep daily usage low-syntax while preserving step discipline.
- `template` uses built‑in task templates (see `tasks_templates_list`) to reduce input verbosity.
- `plan_template` is allowed only when creating a new plan via `plan_title`.
- UX recovery: if `plan`/`parent` is provided but does not start with `PLAN-`, it is treated as a plan title (equivalent to `plan_title`) to avoid common LLM mis-wirings.
- If `plan`/`parent` and `plan_title` are both provided, `plan_title` acts as a consistency check: it must match the referenced plan’s stored title, otherwise the call fails with `INVALID_INPUT` (prevents silent mis-targeting).
- If neither `plan` nor `parent` nor `plan_title` is provided and the workspace focus is a plan, that plan is used as the implicit parent (focus-first DX).
- If neither `plan` nor `parent` nor `plan_title` is provided and the workspace focus is a task, the macro uses that task’s parent plan (stay-in-plan DX).
- If no implicit parent can be derived from focus, the macro uses the first plan with title `"Inbox"` if it exists; otherwise it creates a new `"Inbox"` plan.
- If `think` is provided, it is forwarded to the bootstrap pipeline and the response may include `think_pipeline` as confirmation.
- Reasoning-first default for principal tasks: if `template="principal-task"` and `reasoning_mode` is omitted, the server defaults to `reasoning_mode="strict"` (opt-out by passing an explicit mode).
- Reasoning-first default for principal tasks: if `template="principal-task"` and `think` is omitted, the server seeds a minimal `think.frame` card derived from `task_title` and `description` (deterministic, low-noise).
- The returned `resume` is a `tasks_resume_super` snapshot:
  - If `view` is omitted, defaults to `view="smart"` (relevance-first, cold archive).
  - `view` is forwarded to the underlying resume call, so callers can opt into `explore`/`audit` explicitly.
- If `agent_id` is present, it is forwarded to the resume call for **lease-aware** actions and audit metadata (durable memory visibility does not depend on `agent_id`).

### `tasks_macro_delegate`

One‑call делегирование: создать задачу (principal defaults) → засеять `cockpit` (pinned canon note) → вернуть супер‑резюме.

Input: `{ workspace, agent_id?, plan?, parent?, plan_title?, plan_template?, task_title, description?, anchor?, anchor_kind?, cockpit?, job?, job_kind?, job_priority?, reasoning_mode?, view?, refs?, resume_max_chars? }`

Output: `{ task_id, plan_id?, cockpit, job?, resume }`

Semantics:

- Intent: make long research work resumable-by-meaning after `/compact` or restarts without rereading code.
- Task creation uses the same bootstrap rules as `tasks_macro_start` (focus-first implicit parent plan, deterministic templates).
- Reasoning-first default: if `reasoning_mode` is omitted, defaults to `reasoning_mode="strict"` (opt-out by passing an explicit mode).
- Anchoring:
  - If `anchor` is provided, it is used as the anchor id (must be a valid `a:<slug>` or resolvable via alias mapping).
  - If `anchor` is omitted, the server derives an anchor id from `task_title` (heuristic: prefix before `:`; deterministic ascii slugify; fallback `a:core`).
  - If the anchor does not exist, it is auto-created with `kind=anchor_kind` (default: `component`) and a title derived from `task_title`.
- Cockpit seeding:
  - The server writes a single pinned note card into the task reasoning scope (shared memory), tagged with the anchor id and `v:canon`.
  - If `cockpit` is omitted, the content uses the deterministic `initiative` template derived from `task_title` + `description`.
  - The response includes `cockpit.card_id` (stable jump handle, `CARD-*`) and `cockpit.anchor_id`.
- Delegation job (logistics, not execution):
  - By default, the server creates a `JOB-*` record linked to the new task + anchor.
  - `job=false` disables job creation (useful when you want only the task+cockpit).
  - `job_kind` and `job_priority` let callers annotate what runner should pick it up (execution remains external; see `DELEGATION.md`).
  - `tasks_snapshot` surfaces one active job (`RUNNING` > `QUEUED`) in the first line as `job=JOB-*` to keep delegation visible without adding noise.
- Budget UX:
  - Portal views always keep a stable `ref=<id>` handle in the first (state) line.
  - When `refs=true` (explicit nav-mode) or `delta=true`, portal views may include additional bounded `REFERENCE:` lines and a bounded `open ... max_chars=8000` jump.

## Delegation jobs (v0.6)

Delegation is tracked as durable `JOB-*` entities, but **execution is external** (see `DELEGATION.md`).

### `tasks_jobs_create`

Create a new job record for delegated work.

Input: `{ workspace, title, prompt, kind?, task?, anchor?, priority?, meta? }`

Output: `{ job_id, status, title, task?, anchor?, created_at_ms, updated_at_ms }`

Semantics:

- `status` starts as `QUEUED`.
- `task` (optional) links the job to a task (often the one created by `tasks_macro_delegate`).
- `anchor` (optional) links the job to meaning-map anchors for resume-by-meaning.
- `priority` is normalized case-insensitively:
  - canonical values: `LOW|MEDIUM|HIGH`
  - accepted synonyms: `normal` → `MEDIUM`
- This tool only creates the job record; it does not run anything.

### `tasks_jobs_list`

List jobs with bounded output.

Input: `{ workspace, status?, task?, anchor?, limit?, max_chars?, fmt? }`

Output: `{ runner_status, jobs, count, has_more, truncated }`

Semantics:

- Deterministic ordering: `updated_at_ms DESC`, tie-break by `job_id ASC`.
- `limit` is clamped to a safe maximum (server-defined).

### `tasks_jobs_radar`

List **active** jobs with a low-noise “attention” hint and the latest meaningful update.

Input: `{ workspace, status?, task?, anchor?, limit?, runners_limit?, runners_status?, offline_limit?, stale_after_s?, reply_job?, reply_message?, reply_refs?, max_chars?, fmt? }`

Output: `{ runner_status, runner_leases, runner_leases_offline?, runner_diagnostics?, jobs, count, has_more, truncated }`

Where each job row includes:

- `job_id`, `status`, `title`, `task?`, `anchor?`, `updated_at_ms`, ...
- `last?`: latest meaningful event (bounded fields: `{ ref, seq, ts_ms, kind, message, refs }`)
- `attention`: `{ needs_manager, needs_proof, has_error, stale }` (booleans)

Semantics:

- If `status` is omitted, defaults to `status IN (RUNNING, QUEUED)` (active jobs).
- Deterministic ordering (inbox-first): jobs that need attention are surfaced first:
  - `has_error` (`!`) first,
  - then `needs_manager` (`?`),
  - then `stale` (`~`),
  - then `RUNNING` before `QUEUED`,
  - then `updated_at_ms DESC`,
  - tie-break by `job_id ASC`.
- `last` selection:
  - prefer the newest event where `kind != "heartbeat"` and `message` does not start with `runner:`
  - fallback to the newest non-heartbeat event, else the newest event
- Attention:
  - These attention flags are computed from a bounded recent event scan (server-defined), not from the full history.
  - `needs_manager=true` when the most recent `question` event is newer than the most recent `manager` message
    (sticky across intervening `progress` noise)
  - `needs_proof=true` when the most recent `proof_gate` event is newer than the most recent “proof satisfaction” event:
    - `checkpoint`, or
    - a `manager` message that contains at least one stable ref (either explicitly via `refs[]` or salvaged from the message body).
    This is sticky across intervening `progress` noise and is intended to reduce “DONE without evidence” loops.
  - `has_error=true` when the most recent `error` event is newer than the most recent `checkpoint`
    (sticky until a later checkpoint is recorded)
  - `stale=true` when `status == "RUNNING"` and the explicit job claim lease is expired:
    - `claim_expires_at_ms <= now_ms`, or
    - `claim_expires_at_ms` is missing (treated as expired, back-compat).
    `stale_after_s` is kept for backward-compat only and is not used for default stale detection.
- Budget UX: must prefer keeping `JOB-*` ids and `seq` over long messages.

Runner bootstrap hint (DX):

- When at least one job is `QUEUED` and the runner is `offline`, the server may include `runner_bootstrap` to help users avoid
  “jobs stay queued” confusion:
  - `runner_bootstrap.cmd`: copy/paste command to start `bm_runner` for this store/workspace.
  - This is a hint only; execution remains external and deterministic.

Runner status (DX, no-heuristics):

- The server includes `runner_status` so inbox users can see runner `offline|idle|live` with no ambiguity.
- `runner_status` is derived from explicit runner leases (see `tasks_runner_heartbeat` in `docs/contracts/DELEGATION.md`),
  not inferred from job events.
- Fields (v1.7):
  - `status`: `offline|idle|live` (derived)
  - `live_count`, `idle_count`, `offline_count`: counts (derived from lease expiry)
  - `runner_id?`, `active_job_id?`, `lease_expires_at_ms?`: a representative runner (for quick UX only; not authoritative)

Runner leases (DX, multi-runner):

- `runner_leases.runners[]` is a bounded list of active (non-expired) runner leases for this workspace.
- Deterministic ordering: `live` runners first, then `idle`; tie-break by `lease_expires_at_ms DESC`, then `runner_id ASC`.
- `runner_leases.has_more=true` means more active leases exist beyond the bound.
- `runners_limit` and `runners_status` let callers increase the bound or filter to `idle|live` when diagnosing many runners.

Offline runner leases (DX, multi-runner) (v1.9):

- `runner_leases_offline.runners[]` is a bounded list of **recently expired** runner leases (`lease_expires_at_ms <= now_ms`).
- Deterministic ordering: `updated_at_ms DESC`, tie-break by `runner_id ASC`.
- This is intended to make “who went offline” visible at a glance without opening anything.
- `offline_limit=0` disables this list (keeps radar minimal for single-runner setups).
- The list may be omitted when there are no expired leases or when budgets are tight.

Runner diagnostics (DX, multi-runner conflicts) (v1.7):

- `runner_diagnostics.issues[]` is a bounded list of explicit consistency problems between:
  - runner liveness leases (`tasks_runner_heartbeat`), and
  - job claim leases (`claim_expires_at_ms`) / job ownership (`job.runner`).
- Each issue includes at least: `severity` (`error|warn|stale`), `kind`, `message`, and optionally `runner_id`, `job_id`.
- This is not inferred from logs or timing heuristics; it is based on explicit lease state and ownership records.

Reply shortcut (DX):

- If `reply_job` and `reply_message` are provided, the server first appends a manager message event to that job
  (same semantics as `tasks_jobs_message`), then returns the updated radar list.
- `reply_refs` are optional stable refs attached to the manager message (for navigation).

### `tasks_jobs_open`

Open a job in a portal-friendly view (status + spec + recent events).

Input: `{ workspace, job, include_prompt?, include_events?, include_meta?, max_events?, before_seq?, max_chars?, fmt? }`

Output: `{ job, meta?, events?, truncated }`

Semantics:

- Default is low-noise: show `job` + the most recent events only.
- If `max_chars` is tight, the tool must prefer keeping stable ids/refs over long bodies.
- Event objects include a stable `ref` field (`JOB-...@seq`) for no-hunt navigation.
- `meta` is only included when `include_meta=true` and is intended for runner-side budgets/config (never required for reading jobs).
- `before_seq` enables paging older history without dumping huge event logs. When provided, the server returns events strictly older than `before_seq` (deterministic, bounded).

### `tasks_jobs_tail`

Follow a job event stream **incrementally** (supervision without losing place).

Input: `{ workspace, job, after_seq?, limit?, max_chars?, fmt? }`

Output: `{ job_id, after_seq, next_after_seq, events, has_more, truncated }`

Semantics:

- Returns events strictly newer than `after_seq` (`seq > after_seq`).
- Deterministic ordering: `seq ASC` (oldest → newest).
- `next_after_seq` is the greatest `seq` returned (or equals `after_seq` when no events returned).
- When `limit` truncates, `has_more=true` and callers should repeat with `after_seq=next_after_seq`.
- Budget UX: must keep `job_id`, `after_seq`, `next_after_seq` even under tight `max_chars`.

### `tasks_jobs_claim`

Claim a queued job for execution by an external runner.

Input: `{ workspace, job, runner_id, lease_ttl_ms?, allow_stale? }`

Output: `{ job }`

Semantics:

- Transition: `QUEUED` → `RUNNING`.
- If the job is not `QUEUED`, the call fails with `CONFLICT` and a recovery hint (open the job, decide to cancel/requeue).
- `runner_id` is required and becomes `job.runner` (audit + routing; memory visibility does not depend on it).
- The server returns the updated `job.revision`, which becomes the runner’s claim token (`claim_revision`).
- The server sets `job.claim_expires_at_ms = now_ms + lease_ttl_ms` (time-slice / reclaim boundary).
- If `allow_stale=true`, a runner may reclaim a job already in `RUNNING` state when the claim lease is expired:
  - expiry is defined by `job.claim_expires_at_ms <= now_ms`
  - this supports crash recovery for multi-hour runs without job-event heuristics.

### `tasks_jobs_message`

Send a manager message to a job (for supervision / course correction).

Input: `{ workspace, job, message, refs?, fmt? }`

Output: `{ event }`

Semantics:

- Allowed when job status is `QUEUED` or `RUNNING`.
- `message` should be short (no logs); put larger context into stable `refs` (`CARD-*`, `notes@seq`, `TASK-*`, `a:*`).
- If `refs` are omitted/empty, the server may deterministically salvage a bounded set of refs from the message body to reduce “hunt for proof” loops:
  - receipts (`CMD:`/`LINK:`),
  - stable ids (`CARD-*`, `TASK-*`, `PLAN-*`, `JOB-*`, `notes@*`, `a:*`).
  This is metadata extraction only (no execution).
- Messages are intended to be injected into the next runner slice as a bounded “job thread”.
- `fmt=lines` renders a low-noise BM-L1 view (copy/paste-friendly).

### `tasks_jobs_report`

Append a bounded progress event to a running job.

Input: `{ workspace, job, runner_id, claim_revision, message, kind?, percent?, refs?, meta?, lease_ttl_ms? }`

Output: `{ event }`

Semantics:

- `message` should be short and stable (no logs).
- `refs` may include stable handles such as `TASK-*`, `CARD-*`, or `doc@seq`.
- `kind` allows runners to separate `heartbeat` vs `progress` vs `checkpoint` in a low-noise way (bounded event log).
- `kind=proof_gate` is reserved for runner quality gates (e.g. “DONE requires proof refs”). It is **attention-worthy** but does not imply `needs_manager`.
- `kind=heartbeat` is *coalesced* by default (long-running runners should not create unbounded heartbeat spam).
- The server rejects reports when `(runner_id, claim_revision)` does not match the current job row (prevents “zombie runner” writes after reclaim).
- Each report renews the claim lease: `job.claim_expires_at_ms = now_ms + lease_ttl_ms`.

### `tasks_jobs_complete`

Complete a job with a final status and stable references to the real knowledge artifacts.

Input: `{ workspace, job, runner_id, claim_revision, status, summary?, refs?, meta? }`

Output: `{ job }`

Semantics:

- `status` must be one of `DONE|FAILED|CANCELED`.
- `refs` should point to where the evidence/changes live (e.g. cockpit `CARD-*`, task id, notes doc).
- DX salvage: if `refs` is empty and `status=DONE`, the server may **conservatively salvage** stable proof-like references from `summary`
  (e.g. `CMD:` / `LINK:` lines or strong command-looking bullets) to reduce accidental proof-gate loops. This does not override explicit `refs`.
- Navigation: the server ensures the job id (`JOB-*`) is present in the completion event refs when possible (bounded).
- The server rejects completion when `(runner_id, claim_revision)` does not match the current job row (prevents double-completion after reclaim).

### `tasks_jobs_requeue`

Requeue a job for another attempt.

Input: `{ workspace, job, reason?, refs?, meta? }`

Output: `{ job, event }`

Semantics:

- Transition: `FAILED|CANCELED|DONE` → `QUEUED` (creates a new event for auditability).
- Intended for long-running / flaky delegated work where retries are expected.
- The runner should treat requeued jobs as fresh attempts and must not assume prior state.

### `tasks_macro_close_step`

One‑call закрытие шага: подтвердить чекпойнты → закрыть → вернуть супер‑резюме.

Input: `{ workspace, agent_id?, task?, path?|step_id?, checkpoints?, expected_revision?, note?, proof?, override?, view?, resume_max_chars? }`

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
  - `view` is forwarded to the underlying resume call; use `audit` when you explicitly want drafts / full archive visibility.
  - If `agent_id` is present, it is forwarded to the resume call for **lease-aware** actions and audit metadata (durable memory visibility does not depend on `agent_id`).
- If the target task has `reasoning_mode="strict"`, the macro may fail with `error.code="REASONING_REQUIRED"` when the
  reasoning engine signals missing discipline for the current step (e.g. `BM4_HYPOTHESIS_NO_TEST`, `BM10_NO_COUNTER_EDGES`).
  The response should include portal-first recovery suggestions (typically `think_card`).
- To avoid dead-ends, callers may pass `override={ reason, risk }` to bypass the strict reasoning gate for this close attempt.
  The server must record a durable note (reason + risk + missing signals) and emit `WARNING: STRICT_OVERRIDE_APPLIED`.
  `override` does **not** bypass checkpoint or proof requirements.
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

Input: `{ workspace, agent_id?, plan?, parent?, plan_title?, plan_template?, task_title, description?, template?, steps[]?, think?, reasoning_mode? }`
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
- If top-level `agent_id` is provided and `think.agent_id` is omitted, the server may forward `agent_id` to `think_pipeline` for audit metadata; durable memory does not depend on `agent_id`.

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
