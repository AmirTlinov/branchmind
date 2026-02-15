# Contracts — v1 Command Registry (SSOT)

This document is the **stable index of v1 commands** (`cmd`). It is the single source of truth
for public-facing operations, with schema discovery via `system` → `schema.get`.

## Command index {#cmd-index}

Advanced/legacy commands may share this anchor. Use `system` → `schema.get(cmd)` for exact
arguments, examples, and budget defaults.

---

## Shared reference formats

### CODE_REF (strict)

Canonical token (no spaces):

- `code:<repo_rel>#L<start>-L<end>@sha256:<64hex>`

Validation rules:

- `repo_rel` MUST be a non-empty repo-relative path.
- `start` / `end` MUST be 1-based integers with `start <= end`.
- `sha256` MUST be exactly 64 hex chars (input may be mixed-case; stored canonicalized to lowercase).
- Invalid CODE_REF tokens are rejected with `INVALID_INPUT` (fail-closed).

Compatibility notes:

- This is documentation-only clarification of current runtime behavior (no contract-breaking change).
- This strict shape is the canonical contract for `scout_context_pack.code_refs[]` and `anchors[*].code_ref`.
- Regex equivalent: `^code:[^#]+#L[1-9][0-9]*-L[1-9][0-9]*@sha256:[0-9a-fA-F]{64}$`

Examples:

- `code:crates/mcp/src/support/code_ref.rs#L36-L84@sha256:0000000000000000000000000000000000000000000000000000000000000000`
- `code:crates/mcp/src/support/artifact_contracts/pipeline_v2.rs#L259-L340@sha256:1111111111111111111111111111111111111111111111111111111111111111`
- `code:docs/contracts/V1_COMMANDS.md#L1-L60@sha256:2222222222222222222222222222222222222222222222222222222222222222`

---

## system.schema.get

Return the schema bundle for a command (`args_schema`, `example_minimal_args`,
`example_valid_call`, `doc_ref`).

Notes:
- Runtime is **fail-open**: the server always returns the schema bundle even if local docs are unavailable.
  Docs drift is enforced by CI/test guards (not in the agent UX loop).

## system.schema.list

List commands (and minimal schema hints) to discover the right `cmd` for `system.schema.get`.

Inputs (selected):

- `portal` (string, optional): filter by portal (`tasks|jobs|think|graph|vcs|docs|workspace|system`).
- `prefix` (string, optional): `starts_with` filter (e.g. `tasks.`).
- `q` (string, optional): case-insensitive substring filter (applied after `portal/prefix`).
- `mode` (string, optional): `golden` (default) or `all` (`names|compact` kept for compatibility).
- `offset` / `limit` (int, optional): pagination (limit is clamped).

Output (selected):

- `{ schemas:[...], pagination:{...} }`
- In `mode=all` (`compact` compatibility), each schema row includes:
  - `required`: always-required args (workspace excluded for v1 envelope parity).
  - `required_any_of`: alternative required sets for disjunction schemas (`oneOf`/`anyOf`),
    useful for commands like `jobs.claim`.

Notes:

- Intended for discovery (“what is the real cmd name?”) and for driving `system.schema.get`.

## system.cmd.list

List all registered `cmd` names (SSOT registry).

Inputs (selected):

- `q` (string, optional): case-insensitive substring filter (applied after `prefix`).
- `prefix` (string, optional): `starts_with` filter (e.g. `tasks.`).
- `mode` (string, optional): `golden` (default) or `all` (`names|compact` kept for compatibility).
- `offset` (int, optional): pagination offset.
- `limit` (int, optional): page size.

Notes:

- By default, returns only golden-ops commands; use `mode=all` for full registry.

## system.tools.list

List the v1 portal surface (10 tools) and their golden ops (as advertised in `tools/list`).

Output (selected):

- `{ tools:[{tool,description,ops}], examples:[...], quickstart_schema_hint:{...}, notes:[...] }`

Notes:

- `quickstart_schema_hint` is a structured “shape hint” for `system.quickstart` (clients should prefer it over parsing `notes[]`).
- `notes[]` may include extra quickstart UX hints such as `result.defaults` and `recipes[].uses_defaults` (for client UI badges).

## system.quickstart

Print 3–5 ready-to-run “recipes” strictly for a chosen portal (tasks/jobs/workspace/system/...).

Inputs (selected):

- `portal` (string, required): portal/tool name (see `system.tools.list`).
- `limit` (int, optional): max recipes returned (default 5, clamped 1..5).

Output (selected):

- `{ title, portal, workspace_selected, workspace_selected_source, defaults:{default_branch,checkout_branch}, recipes:[...], truncated }`

Notes:

- Recipes are copy/paste-friendly tool calls and are also returned as executable `actions[]`.
- Every quickstart action is expected to run as-is under its own `budget_profile` (copy/paste reliability gate).
- `defaults.checkout_branch` is derived from the workspace checkout when available (falls back to `default_branch` in recipes).
- Each `recipes[]` item may include `uses_defaults: [..]` listing which `defaults.*` values are used (for UI badges).
- This command is also available as a tool op: `system(op=quickstart args={portal:"tasks"})`.

## system.exec.summary

One-command cross-portal preset: **execution summary + critical regressions**.

Inputs (selected):

- `include_tasks` / `include_jobs` (bool, optional): enable providers (default `true/true`).
- `task` / `plan` / `target` (string, optional): focus override for task-level summary.
- `anchor` (string, optional): scope hint for jobs summary.
- `jobs_view` (`smart|audit`, optional): jobs control-center view (default `smart`).
- `jobs_limit` (int, optional): jobs scan limit for control-center (default `20`).
- `stall_after_s` (int, optional): jobs stalled threshold override.

Output (stable keys):

- `now`: compact next-engine pulse (`headline`, `focus`, `state_fingerprint`).
- `summary.tasks`: task execution summary (from `tasks.exec.summary`).
- `summary.jobs`: jobs execution/proof/inbox health (from `jobs.control.center`).
- `critical_regressions[]`: merged high-severity regressions/attention.
- `blockers[]`: merged blockers from task radar + P0 jobs attention.
- `provider_health`: per-provider status (`ok|error|skipped`).

Notes:

- Fail-soft by provider: if one provider fails, others still return and `provider_health` marks the failure.
- Designed for teamlead loops where one call should answer “what is happening now?” and “what can break us?”.

## system.ops.summary

Return a small, low-noise summary of the v1 UX surface:

- tool surface count + names (must be 10),
- golden ops count (as advertised in `tools/list`),
- cmd registry count (and cmd-by-domain counts),
- unplugged ops (if any) to detect “advertised but not dispatchable” drift.

## system.migration.lookup

Map old tool name → `cmd` and return a minimal call example.

## system.storage

Low-level storage introspection (legacy `storage`). Intended for debugging / internal ops.

## system.init

Initialize a workspace (legacy `init`).

## system.help

Help / quick reference (legacy `help`).

## system.tutorial

Guided onboarding (actions-first).

Inputs (selected):

- `limit` (int, optional): max onboarding steps returned (default 3).
- `max_chars` (int, optional): max chars for the tutorial summary text.

Output (selected):

- `{ title, summary, steps:[...], truncated }`

Notes:

- Steps follow the golden path: `status → tasks.macro.start → think.trace.sequential.step → tasks.snapshot`.
- `actions[]` includes executable calls for each returned step (bounded by `limit`).
- If `workspace` is not set, actions rely on the default workspace (or call `workspace.use` first).
- `truncated=true` when `limit` or `max_chars` cuts the tutorial output.
- This command is also available as a tool op: `system(op=tutorial args={})`.

## system.skill

Skill discovery / info (legacy `skill`).

## system.diagnostics

Diagnostics snapshot (legacy `diagnostics`). Intended for debugging / internal ops.

---

## workspace.use

Switch the active workspace for the session.

## workspace.reset

Clear the workspace override and return to the default/auto workspace.

## workspace.list

List known workspaces and show the most-recently used bound filesystem path (when available).

Output markers (selected):

- `selected_workspace` / `active_workspace`: what is active right now.
- `selected_workspace_source`: why it is active (`workspace_override|default_workspace|none`).
- `requested_workspace`: workspace applied to this call (after normalization/injection).

---

## tasks.plan.create

Create a plan or task (legacy `tasks_create`).

## tasks.plan.decompose

Add steps to a task/plan (legacy `tasks_decompose`).

## tasks.slices.propose_next

Propose **exactly one** next Slice‑Plans v1 spec (read‑only; does not write to store).

Notes:
- Always returns a single bounded `slice_plan_spec` (one slice, not “the whole plan”).
- Intended loop: `tasks.slices.propose_next` → (edit `slice_plan_spec`) → `tasks.slices.apply`.
- The response includes `actions[]` with a ready-to-run `tasks.slices.apply` call.

## tasks.slices.apply

Apply one Slice‑Plans v1 spec: creates `slice_id` (`SLC-...`), a slice container task (`slice_task_id`),
and a deterministic 2‑level step tree (**SliceTasks(root) → Steps(children)**).

Fail‑closed (selected):
- `tasks` length must be `3..10`; each `task.steps` length must be `3..10`.
- Slice/task `tests[]` and `blockers[]` must be non‑empty.
- `shared_context_refs[]` must not be duplicated verbatim inside tasks/steps (dedupe guard).

## tasks.slice.open

Open a slice by `slice_id`: returns binding + slice task + parsed `slice_plan_spec` + step tree,
plus ready‑to‑run `jobs.*` `actions[]`.

## tasks.slice.validate

Validate slice plan structure + deterministic step tree + budgets (fail‑closed).

## tasks.planfs.init

Initialize a physical plan package in repo files:

- `docs/plans/<slug>/PLAN.md`
- `docs/plans/<slug>/Slice-1.md`, `Slice-2.md`, …

Notes:
- Source of truth for generation is the current task step tree (`s:0`, `s:1`, … + child steps).
- Optional source switch: `from_plan_spec=true` reads latest `doc_kind=plan_spec` snapshot instead of task tree.
  - Optional selectors: `plan_spec_branch`, `plan_spec_doc` (defaults: task reasoning branch + `plan_spec:<TASK-ID>`).
- Fails closed when the task has no root steps, or required list fields are empty.
- Requires workspace to be bound to a repo root path.

## tasks.planfs.export

Render/export the current task step tree into the same PlanFS layout as `tasks.planfs.init`.

Notes:
- Idempotent renderer: unchanged files are reported under `write.unchanged` and are not rewritten.
- `overwrite=true` updates changed files; `overwrite=false` fails on drift.
- Every successful init/export also persists canonical `plan_spec.v1` snapshot into reasoning docs (`doc_kind=plan_spec`),
  with idempotent append (`status=unchanged|appended`).

## tasks.planfs.import

Import `PLAN.md` + `Slice-*.md` from PlanFS files back into the current task step tree.

Strict fail-closed defaults:
- rejects placeholders (`TODO`, `TBD`, `<fill>`, …) in `Success Criteria` / `Tests` / `Blockers`;
- rejects empty required lists (`Success Criteria`, `Tests`, `Blockers`);
- enforces explicit import budgets (`max_slices`, `max_items_per_list`, `max_tasks_per_slice`, `max_steps_per_task`, `max_file_bytes`).

Notes:
- `apply=true` mutates task steps.

## docs.show / docs.diff / docs.merge (plan_spec)

`doc_kind` now supports `plan_spec` (in addition to `notes` / `trace`):

- `docs.show` returns latest canonical parsed payload under `result.plan_spec`.
- `docs.diff` adds `result.plan_spec_diff` with structural JSON paths (`changed_paths[]`).
- `docs.merge` supports deterministic `plan_spec` merge by appending canonical latest payload from `from` into `into`
  (idempotent when already identical).
- `apply=false` performs strict validation only (dry-run import gate).

## tasks.evidence.capture

Attach proof artifacts/checks to a step or task (legacy `tasks_evidence_capture`).

## tasks.step.close

Confirm checkpoints and close a step (legacy `tasks_close_step`).

## tasks.execute.next

Return NextEngine actions for the current focus.

## tasks.exec.summary

One-command preset: execution summary + critical regressions.

Output (selected):

- `exec_summary`: compact handoff/radar/steps snapshot (from `tasks.handoff`).
- `critical_regressions`: lint issues filtered to severity `error|critical` or `code` containing `REGRESSION`.
- `lint_summary` / `lint_status`: compact lint health counters.

Notes:

- Designed for teamlead “quick pulse” loops where you need both execution state and hard regressions in one call.
- Internally composes `tasks.handoff` + `tasks.lint` under the same target/focus.

## tasks.search

Jump/search for `TASK-*` / `PLAN-*` (and `SLC-*` when Slice‑Plans v1 is enabled) by text
(id/title/description/context) and return openable ids.

Notes:
- Intended to avoid “cmd.list → scroll → copy id” loops.
- The response includes `actions[]` to `open id=...` for each returned hit (bounded by `limit`).

---

## jobs.create

Create a delegation job (legacy `tasks_jobs_create`).

Notes:
- If the runner is offline and autostart is enabled, the server may auto-start `bm_runner`.
- The response may include `runner_autostart` and (when needed) `runner_bootstrap` (copy/paste command).

## jobs.list

List jobs (legacy `tasks_jobs_list`).

## jobs.radar

Low-noise job radar (legacy `tasks_jobs_radar`).

## jobs.control.center

Manager control center: **one call = operational slice + ready actions**.

Output blocks (stable keys):

- `inbox`: attention items (needs manager / proof gate / stale / stalled / errors).
- `execution_health`: runner status summary + stalled/stale counts.
- `proof_health`: proof-gate outstanding jobs (manager should respond with proof refs).
- `team_mesh`: threads + unread counters + dependency edges (when mesh is enabled).
- `jobs`: canonical job rows (with `attention` hints).
- `actions`: copy/paste-ready macro actions (intent-first).
- `defaults`: transparency for SLA + guardrails (timeouts, strict schema flags).

Notes:
- Primary daily driver for teamlead agents: call `jobs.control.center`, then run the first action.
- Uses the same bounded attention heuristics as `jobs.radar`.
- `actions` includes `jobs.mesh.pull` only for threads with `unread>0` (no noisy pull action when inbox is already acknowledged).

## jobs.exec.summary

One-command **meaning-first** jobs pulse (minimal-noise preset).

Output (stable keys):

- `now`: headline + compact counters (`running/queued/inbox/critical`) + runner status.
- `proven`: enabled guardrails (`strict_progress_schema`, `unknown_args_fail_closed`, `high_done_proof_gate`, `wait_stream_v2`, …) and core execution-health counters.
- `critical_regressions` / `critical_regressions_count`: only P0/P1 attention items (bounded by `max_regressions`, default 3).
- `next`: top 1–3 recommended actions (compact metadata), mirrored in top-level `actions[]` for direct execution.
- `source`: always `jobs.control.center` (explicit provenance).

Inputs (optional):

- `view` (`smart|audit`, default `smart`)
- `limit` (default 20)
- `task`, `anchor`
- `stall_after_s`
- `max_regressions` (default 3, clamped to 1..20)
- `include_details` (default `false`) — add deep diagnostics blocks only when explicitly requested.

Notes:
- Designed as the default daily driver for agents that need signal-first UX without control-center payload noise.
- If queue exists and no live runner is detected, returns a high-priority `jobs.runner.start` recovery action.

## jobs.open

Open a job record (legacy `tasks_jobs_open`).

Inputs (selected):

- `include_meta` (bool, optional): include job `meta` JSON.
- `include_events` (bool, optional): include recent event tail (bounded).
- `include_artifacts` (bool, optional): include an artifacts lens:
  - stored `job_artifacts` rows (keys + `artifact_ref`),
  - expected-but-missing artifacts derived from `job.meta.expected_artifacts`,
  - plus copy/paste `actions[]` for reading (`open(id=artifact_ref)` / `jobs.artifact.get`).

## jobs.complete

Mark job completion with artifact-contract guardrails for pipeline jobs.

Inputs (selected):

- `job` (string, required), `runner_id` (string, required), `claim_revision` (int, required), `status` (string, required), `summary` (string).

Contract behavior:

- If `status = \"DONE\"` and `job.meta.expected_artifacts` is set:
  - `summary` must be a valid JSON object (pack root),
  - only one expected artifact key is supported; more than one returns `PRECONDITION_FAILED` with `expected_artifacts>1 not supported; split jobs or use separate artifacts`.
  - key is validated (`scout_context_pack` / `builder_diff_batch` / `validator_report`), then normalized and stored into `job_artifacts`.
  - invalid/missing pack JSON also returns `PRECONDITION_FAILED`.

## jobs.artifact.put

Store a text artifact under a job and return a stable `artifact_ref` (`artifact://jobs/JOB-.../<artifact_key>`).

Notes:
- Primary use: store diffs (`unified_diff`) or other bounded job outputs so gate/apply can enforce budgets
  without reading runner stdout/stderr.
- Stable refs can be read through `open(id=artifact_ref)` in addition to `jobs.artifact.get`.

## jobs.artifact.get

Read a bounded slice of a job artifact by `{ job, artifact_key, offset, max_chars }`.

Notes:

- Returns `artifact.source`:
  - `store` when the artifact is stored in `job_artifacts`;
  - `summary_fallback` when derived from `job.summary` (legacy / pre-materialization).
- When `artifact.source="summary_fallback"`, the response includes a warning `ARTIFACT_FALLBACK_FROM_SUMMARY`.
- New servers may materialize expected artifacts on `jobs.complete(status=DONE)` automatically (fail-closed when the job declares `expected_artifacts`).

## jobs.proof.attach

Attach proof receipts from a job to a task/step (legacy `tasks_jobs_proof_attach`).

Notes:
- Input includes `{ job, task?, step_id?|path?, checkpoint?, artifact_ref?, max_file_bytes? }`.
- The server resolves stable refs from the job (summary/refs + `artifact_ref`) and records evidence.
- Attachments are emitted as `LINK: file://...` (with `sha256` when available) when possible.
- `max_file_bytes` bounds sha256 hashing (default: 64 MiB per file, best-effort).

## jobs.cancel

Cancel a job (QUEUED → CANCELED).

If the job is RUNNING, you have two safe options:
- set `force_running=true` (optionally with `expected_revision`) to cancel RUNNING directly, or
- follow the recovery actions to cancel via `jobs.complete status=CANCELED`.

Notes:
- RUNNING without `force_running` returns `error.code="CONFLICT"` and includes recovery actions (`jobs.open`, `jobs.complete`).
- `expected_revision` is a best-effort race guard (match the current job revision).
- Cancellation self-heals runner leases when `runner_leases.active_job_id` points to the canceled job.
- Use `system` → `schema.get(cmd)` for exact arguments.

## jobs.wait

Wait for a job to reach a terminal status (DONE/FAILED/CANCELED), bounded by `timeout_ms`.

Notes:
- On timeout, returns `success=true` with `result.done=false` and `result.timed_out=true` (not an error).
- Default mode is `stream` (when enabled): returns new `events[]` since `after_seq` and may return early on progress.
- Use `after_seq` + `max_events` (or legacy `limit`) to page events; feed `next_after_seq` into the next call.
- `mode=poll` is a legacy status-only fallback.
- Output includes `{ done, timed_out, waited_ms, job, events, next_after_seq, has_more }` (some fields are omitted in `mode=poll`).
- `timeout_ms` is intentionally capped (currently `<= 25000`) to stay below typical MCP deadlines; for longer waits, loop `jobs.wait` or use `jobs.radar`.
- Use `system` → `schema.get(cmd)` for exact arguments.

## jobs.macro.rotate.stalled

Manager-side one-call self-heal for stalled RUNNING jobs: cancel + recreate.

Notes:
- Detects “stalled” as: lease is still valid (not stale) but no meaningful checkpoint/progress for `stall_after_s` seconds.
- `dry_run=true` previews what would be rotated.
- Rotation preserves prompt + key meta (executor/executor_profile/expected_artifacts/policy/routing) and tags `meta.rotated_from`.

## jobs.macro.respond.inbox

Respond to manager inbox items (questions) with one call (posts `manager` messages).

Notes:
- Use this instead of looping `jobs.message` for each job.
- Intended to clear `needs_manager` attention in `jobs.radar` / `jobs.control.center`.
- If `job`/`jobs[]` are omitted, the macro auto-selects targets from radar (`needs_manager=true`) within bounded `limit` (default `25`).
- For `jobs op=call` envelope usage, omitted `limit` keeps this command default (`25`); budget profiles cap only explicit `limit` values.
- If no matching jobs exist, returns success with `count=0` + `NO_MATCHING_JOBS` warning (fail-open, no write).

## jobs.macro.dispatch.slice

Dispatch a single slice as a job record (create + routing meta).

Notes:
- Convenience wrapper for common “create a job with routing defaults” flow.
- Does not execute anything by itself; runners claim QUEUED jobs out-of-process.

## jobs.macro.dispatch.scout

Dispatch a **scout** stage for one slice (`task + anchor + slice_id + objective`).

Notes:
- `slice_id` may be provided directly **or** via `target_ref`:
  - `target_ref=planfs:<slug>#SLICE-<n>`
  - `target_ref=planfs:<slug>/Slice-<n>.md`
  When `target_ref` is provided, objective/budgets come from PlanFS slice spec and the runner prompt includes a bounded PlanFS slice excerpt.
- Slice-first (default): `slice_id` must have a `plan_slices` binding (created via `tasks.slices.apply`);
  `task` must match the binding `plan_id`; `objective` + budgets are sourced from the stored `slice_plan_spec`.
  If `BRANCHMIND_JOBS_SLICE_FIRST_FAIL_CLOSED=0` (or `--no-jobs-slice-first-fail-closed`), the binding becomes optional
  and the command falls back to args-provided objective/budgets (legacy/unplanned mode).
- Default routing: `executor=claude_code`, `executor_profile=deep`, `model=haiku`.
- Fail-closed model policy: for `executor=claude_code`, scout `model` must be Haiku-family (`contains("haiku")`); for `executor=codex`, scout `model` must be `gpt-5.3-codex`.
- Fail-closed profile policy: for `executor=claude_code`, `executor_profile=xhigh` is rejected (use `fast|deep|audit`).
- Runner prompt enforces context-only output (no code/diff/patch/apply).
- Runner prompt enforces bounded scout extraction (max 12 repo reads), with mandatory dedupe for repeated file+intent pairs.
- Runner executes Claude in strict local mode with `--strict-mcp-config --mcp-config {"mcpServers":{}}` (no user/global MCP/plugin servers).
- Runner executes Codex pipeline roles in strict local mode with `--ephemeral -c mcp_servers={}` (no user/global MCP/plugin servers).
- `code_refs[]` are strict CODE_REF tokens: `code:<repo_rel>#L<start>-L<end>@sha256:<64hex>` (fail-closed at builder/pre-validate gates).
- `anchors[]` are typed for pre-validator lineage: each anchor must include `anchor_type` (`primary|dependency|reference|structural`) + `code_ref`.
- Fail-closed path binding: every `change_hints[].path` must be bound by at least one `code_refs[]` / `anchors[].code_ref` path.
  - File path: exact repo-relative match.
  - Directory path (for example `docs/contracts/`): allowed only when at least one bound CODE_REF path is a descendant of that directory.
- Strict novelty contract rejects duplicated `change_hints(path+intent)`, duplicated `test_hints`, and duplicated `risk_map.risk` entries.
- Scout quality gate is strict: `anchors>=3`, `change_hints>=2`, `test_hints>=3`, `risk_map>=3`, `summary_for_builder>=320 chars`.
- Job metadata persists pipeline lineage (`pipeline_role=scout`, `slice_id`, `max_context_refs`).

## jobs.macro.dispatch.builder

Dispatch a **builder** stage for one slice.

Notes:
- `slice_id` may be provided directly or resolved from PlanFS `target_ref` (same syntax as scout).
- When `target_ref` is used, builder prompt receives bounded PlanFS excerpt + derived slice spec (no copy/paste objective/DoD boilerplate).
- Slice-first (default): `slice_id` must be bound via `tasks.slices.apply` and `tasks.slice.validate` must pass.
  `objective` + budgets are sourced from `slice_plan_spec`; missing DoD fields are auto-filled from the slice spec.
  If `BRANCHMIND_JOBS_SLICE_FIRST_FAIL_CLOSED=0` (or `--no-jobs-slice-first-fail-closed`), binding becomes optional
  (legacy/unplanned mode): builder requires args-provided `objective` + `dod` and skips slice step-tree determinism checks.
- Required inputs: `task`, `slice_id`, `scout_pack_ref`, `objective`, `dod`.
- Output contract is `builder_diff_batch` (stored as structured summary payload).
- `builder_diff_batch.checks_to_run` is required and must be non-empty (at least one explicit verification command).
- Hard pin: `executor=codex`, `executor_profile=xhigh`, `model=gpt-5.3-codex` (fail-closed).
- Default is fail-closed on scout quality: dispatch rejects when deterministic `jobs.pipeline.pre.validate` verdict is not `pass`.
- `strict_scout_mode=true` by default and enforces additional scout freshness/quality guards:
  - stale scout pack is rejected (`scout_stale_after_s`, default `900`),
  - warning-level scout quality drift is rejected,
  - `allow_prevalidate_non_pass=true` is forbidden.
- `context_quality_gate=true` by default (hard-fail on warning-level scout drift, including `CODE_REF_STALE`).
- `input_mode=strict` by default: builder must use provided context only and avoid tool/repo discovery loops.
  Runner-level enforcement: Codex pipeline roles run with `--ephemeral -c mcp_servers={}` (no MCP tool side effects).
- `max_context_requests` (alias over retry limit) is bounded `<=2` to prevent endless ping-pong.
- Escape hatch (only with `strict_scout_mode=false`): `allow_prevalidate_non_pass=true` allows dispatch with warning (`need_more`/`reject`) for controlled experiments only.
- Builder may return `context_request` in `builder_diff_batch` with `changes=[]` to request missing scout context.
  The loop is bounded by `context_retry_count/context_retry_limit` (hard cap `<=2`).

## jobs.macro.dispatch.validator

Dispatch an **independent validator** stage for one slice.

Notes:
- `slice_id` may be provided directly or resolved from PlanFS `target_ref` (same syntax as scout).
- When `target_ref` is used, validator prompt includes bounded PlanFS slice excerpt + derived slice spec.
- Required inputs: `task`, `slice_id`, `scout_pack_ref`, `builder_batch_ref`, `plan_ref`.
- Output contract is `validator_report`.
- Runner uses role-aware soft slice caps (scout 300s, builder/writer 1200s, validator 600s) and role-aware heartbeat cadence (scout 15s, builder/writer 45s, validator 30s); effective values are emitted in checkpoint meta.
- Hard pin: `executor=claude_code`, `executor_profile=audit`, `model=opus-4.6` family (fail-closed).
- Runner executes Claude in strict local mode with `--strict-mcp-config --mcp-config {"mcpServers":{}}` (no user/global MCP/plugin servers).
- Lineage guard: validator parent lineage must not point to the builder job.

## jobs.macro.dispatch.writer

Dispatch a **writer** stage for Pipeline v2 (patch plan only, no filesystem writes).

Notes:
- Required inputs: `task`, `slice_id`, `scout_pack_ref`, `objective`, `dod`.
- Hard precondition: referenced scout job must be `DONE` and pass strict scout contract validation.
- Writer output contract is `writer_patch_pack` in job summary.
- Defaults: `executor=codex`, `executor_profile=xhigh`, `model=gpt-5.3-codex`.

## jobs.pipeline.pre.validate

Deterministic pre-validator for scout output before writer dispatch (Pipeline v2).

Notes:
- Required inputs: `workspace`, `scout_pack_ref`.
- Optional context hints: `task`, `slice_id` (не влияют на валидацию, только на UX/трассировку).
- Returns `verdict.status` = `pass|need_more|reject` plus structured `checks`.
- Uses pure Rust checks (no LLM) to validate completeness/dependencies/patterns/intent coverage.
- Anchor parsing is compatibility-safe: typed v2 anchors are preferred.
- Legacy fallback (compatibility behavior): when typed v2 anchors are absent, pre-validator synthesizes anchors deterministically from legacy payload.
  - Source order is fixed: `anchors[i].code_ref` → `code_refs[i]` → `code_refs[0]`.
  - If `anchors[]` is empty, one synthetic anchor is created per `code_refs[]`.
  - All fallback `code_ref` values are validated as strict CODE_REF tokens (shared format above).
- Compatibility: legacy `cmd="jobs.pipeline.pre_validate"` is accepted and normalized to this command.

## jobs.pipeline.context.review

Fail-closed scout context quality review before builder dispatch.

Notes:
- Required inputs: `workspace`, `task`, `slice_id`, `scout_pack_ref`.
- Optional: `mode=deterministic|haiku_fast` (default `haiku_fast`), `policy=fail_closed`.
- Returns:
  - `scores` (`freshness`, `coverage`, `dedupe`, `traceability`, `semantic_cohesion`),
  - `verdict.status` = `pass|need_more|reject`,
  - `missing_context[]`,
  - deterministic `actions[]` for next stage.
- `policy=fail_closed` rejects warning-level drift (including stale/unverifiable `CODE_REF`).
- `policy=fail_closed` rejects warning-level drift, but keeps `CODE_REF_UNRESOLVABLE`
  (workspace path not bound for sha-check) as warning-only to avoid false hard stops in dry/test contexts.
- Intended as cheap pre-builder gate to reduce tool-churn and rework loops.

## jobs.pipeline.cascade.init

Initialize Pipeline v2 cascade session (`scout -> pre_validate -> writer -> post_validate -> apply`).

Notes:
- Creates the first stage dispatch and returns durable `session` state.
- Session is deterministic JSON; can be resumed/advanced without hidden state.

## jobs.pipeline.cascade.advance

Advance an existing Pipeline v2 cascade session by event.

Notes:
- Input includes `session_json`, `event`, optional `hints`, optional `job_id`.
- Returns updated session + computed action; fails closed on invalid state/event.

## jobs.macro.enforce.proof

Acknowledge a proof gate by posting a `manager` message that includes proof refs (`LINK:`/`CMD:`/`FILE:`).

Notes:
- Intended to clear `needs_proof` attention when a runner emits `proof_gate`.
- If `job`/`jobs[]` are omitted, the macro auto-selects targets from radar (`needs_proof=true`) within bounded `limit` (default `25`).
- For `jobs op=call` envelope usage, omitted `limit` keeps this command default (`25`); budget profiles cap only explicit `limit` values.
- If no matching jobs exist, returns success with `count=0` + `NO_MATCHING_JOBS` warning (fail-open, no write).

## jobs.macro.sync.team

Publish a shared task plan delta into the team mesh thread (`task/<TASK-ID>`).

Notes:
- Bridge between task planning and threaded collaboration (mesh).

## jobs.pipeline.ab.slice

A/B orchestrator for scout quality comparison (`weak` vs `strong`) with fail-closed downstream flow.

Notes:
- Required inputs: `task`, `anchor`, `slice_id`, `objective`.
- Policy is fixed to `fail_closed`.
- Defaults:
  - `variant_a=weak`, `variant_b=strong`;
  - both scout variants dispatch with `executor=claude_code`, `model=haiku`, `executor_profile=deep`;
  - `weak` => `quality_profile=standard`, `novelty_policy=warn`;
  - `strong` => `quality_profile=flagship`, `novelty_policy=strict`.
- Downstream builder contract (for both variants): `executor=codex`, `model=gpt-5.3-codex`, `executor_profile=xhigh`.
- For each variant, the command prepares/dispatches scout stage and returns continuation actions for builder stage.
- `dry_run=true` returns deterministic planned variants/actions without creating jobs.
- If both `validator_report_ref_a` and `validator_report_ref_b` are provided, the command computes comparison metrics:
  - `plan_fit_score` (A/B/delta),
  - `rework_actions` (A/B/delta),
  - `reopen_rate` (A/B/delta),
  - plus preference decision (`prefer_a|prefer_b|inconclusive`).

## jobs.pipeline.gate

Lead gate for one slice: consume stage artifact refs and return `approve|rework|reject`.

Output (stable keys):
- `decision`: `approve|rework|reject`
- `decision_ref`: deterministic gate artifact ref with lineage/revision payload  
  (`artifact://pipeline/gate/<task>/<slice>/<decision>/builder/<JOB>/validator/<JOB>/rev/<n>`)
- `reasons[]`: fail-closed rationale
- `actions[]`: next recommended command package

Notes:
- Slice-first (default): `slice_id` must have a `plan_slices` binding (via `tasks.slices.apply`).
  If `BRANCHMIND_JOBS_SLICE_FIRST_FAIL_CLOSED=0`, binding becomes optional (legacy/unplanned mode).
- Hard contract checks for `scout_context_pack`, `builder_diff_batch`, `validator_report`.
- Publishes pipeline transition bus messages (`scout_ready`, `builder_ready`, `validator_ready`, `gate_decision`).
- Rejects non-independent validator lineage.
- Slice budgets enforcement (`max_files`, `max_diff_lines`) is controlled by `BRANCHMIND_SLICE_BUDGETS_ENFORCED`
  (default: enabled).

## jobs.pipeline.apply

Fail-closed apply for one slice after gate approval.

Notes:
- Requires `decision_ref` + `builder_batch_ref` + `expected_revision`.
- Slice budgets enforcement is controlled by `BRANCHMIND_SLICE_BUDGETS_ENFORCED` (default: enabled) and reads
  diff artifacts via `jobs.artifact.get`.
- Rejects when decision is not `approve`, when builder revision mismatches, or when validator lineage is invalid.
- Emits `pipeline_apply` bus transition message on success.

## jobs.mesh.publish

Publish a message into a mesh thread (at-least-once + idempotency).

Notes:
- Requires `idempotency_key` for safe retries.
- Validates structured `CODE_REF` in `refs[]`:
  - `code:<repo_rel>#L<start>-L<end>@sha256:<64hex>` (strict format, see **CODE_REF (strict)** above)
  - drift is accepted with `CODE_REF_STALE` warning (message still stored).

## jobs.mesh.pull

Pull messages from a thread after a cursor (`after_seq`), bounded by `limit`.

Notes:
- Cursor paging: feed `next_after_seq` into the next call.

## jobs.mesh.ack

Ack a cursor for a consumer (idempotent, monotonic).

## jobs.mesh.link

Publish a deterministic dependency edge between threads (stored as a `link` message in `workspace/main`).
- Typical flow: `jobs.radar(stall_after_s=600)` → `jobs.macro.rotate.stalled(stall_after_s=600, limit=5)` → `jobs.radar()`.

## jobs.runner.start

Explicitly start the first-party `bm_runner` for the workspace (best-effort).

Notes:
- This is allowed because `bm_mcp` may auto-start the first-party runner (see `DELEGATION.md`).
- Runtime is **fail-open**: on failure, the server returns `runner_bootstrap` (copy/paste command)
  and emits `warnings[]` with `code="RUNNER_START_FAILED"`.

## jobs.runner.heartbeat

Runner heartbeat + capabilities (legacy `tasks_runner_heartbeat`).

---

## think.knowledge.\* (removed)

Knowledge cards are removed from v1 by design.

All commands in this namespace are unavailable:

- `think.knowledge.upsert`
- `think.knowledge.key.suggest`
- `think.knowledge.query`
- `think.knowledge.search`
- `think.knowledge.recall`
- `think.knowledge.lint`
- `think.note.promote`

When called, server returns `UNKNOWN_CMD` + recovery text:

- "Use repo-local skills (.agents/skills/**) and PlanFS docs; knowledge removed by design."

## think.atlas.suggest

Suggest a directory-based atlas (mass onboarding helper): propose anchors bound to key repo paths.

Notes:
- Uses the workspace `bound_path` as `repo_root` by default (or accepts an explicit `repo_root`).
- Default `granularity=depth2`: top-level dirs + one level inside containers (`crates|apps|services|packages|libs`).
- Returns an `actions[]` item that applies the proposal via `think.macro.atlas.apply`.

## think.macro.atlas.apply

Apply an atlas proposal: upsert anchors and bind them to repo paths (`bind_paths` → `path:<repo_rel>` refs).

Notes:
- Safe-by-default: merges against existing anchors to avoid erasing refs/aliases/depends_on.
- Atomic by default (`atomic=true`): either all anchors are applied or none.

## think.atlas.bindings.list

List path → anchor bindings (transparent navigation index; no “magic”).

Notes:
- Use `prefix` to filter by repo area (e.g. `crates/`).
- Use `anchor` to inspect all bindings owned by one anchor id.

## think.macro.counter.hypothesis.stub

One-shot strict reasoning helper: create a counter-hypothesis (tagged `counter`) that blocks `args.against`,
**and** a test stub that supports the counter-hypothesis.

Notes:
- Designed to satisfy strict discipline in fewer round-trips:
  - `BM10_NO_COUNTER_EDGES` (adds a blocking counter-position),
  - `BM4_HYPOTHESIS_NO_TEST` for the counter-hypothesis (creates the supporting test stub).
- Scope is the same as `think.card`: supports `target`/checkout scoping and optional `step` (both cards become step-scoped).
- If `args.counter` / `args.test` are omitted, deterministic defaults are used (titles incorporate `args.label` when provided).
- The macro enforces `counter.type="hypothesis"` and `test.type="test"`, and ensures the counter card has the `counter` tag.
- Idempotency: pass explicit `card.id` values inside `args.counter` / `args.test` if you need repeat-safe writes.

## think.reasoning.seed

Seed a reasoning frame/hypothesis template (legacy `think_template`).

## think.reasoning.pipeline

Run the reasoning pipeline (legacy `think_pipeline`).

## think.trace.sequential.step

Append one structured sequential trace checkpoint (golden strict-reasoning primitive).

Notes:
- Canonical command for strict gate trails before `tasks.macro.close.step(checkpoints="gate")`.
- Recommended metadata for step-scoped trails:
  - `meta.step_id`
  - `meta.step_tag`
  - `meta.checkpoint="gate"`
- Content should stay compact and non-sensitive (checkpoint status, not full private CoT dump).

## think.idea.branch.create

Create an idea branch + capsule card (legacy `macro_branch_note`).

## think.idea.branch.merge

Merge an idea branch + graph state (v1 custom).

---

## graph.apply

Apply graph operations (legacy `graph_apply`).

## graph.query

Query graph view (legacy `graph_query`).

## graph.merge

Merge graph changes (legacy `graph_merge`).

Notes:

- **Resolved conflicts do not re-surface.** Once a conflict is resolved (`status="resolved"`), subsequent
  `graph.merge` calls treat it as handled even if the underlying divergence still exists (e.g. `use_into`),
  preventing “infinite conflict loops”.
- Result counters:
  - `conflicts_detected`: diverged candidates that produced an **open/preview** conflict in the response.
  - `conflicts_created`: new conflict rows inserted into storage (can be `0` in `dry_run=true`, or when
    conflicts already exist).

---

## vcs.branch.create

Create a branch (legacy `branch_create`).

---

## docs.list

List docs for a branch/ref (legacy `docs_list`).

## docs.show

Show document entries (legacy `show`).

## docs.diff

Diff a document between branches (legacy `diff`).

## docs.merge

Merge document entries between branches (legacy `merge`).

## docs.transcripts.search

Search transcripts under `root_dir` for `query` and return bounded matches + openable refs.

## docs.transcripts.digest

Scan transcripts under `root_dir` and return a bounded digest (0..`max_items`) per session.

**Actions on success (v1 UX):**

- May include an `docs` action calling `docs.transcripts.open` for the newest digest item.
- When scan budgets are too tight and no digest items are found, returns warning
  `TRANSCRIPTS_SCAN_TRUNCATED` / `TRANSCRIPTS_MAX_FILES_REACHED` and includes retry actions:
  - retry with a larger scan budget (`max_files`, `max_bytes_total`)
  - optional fallback: retry with `mode="last"` for faster orientation

## docs.transcripts.open

Open a bounded window around a transcript ref (`path` + `line` or `byte`) and return message entries.

**Actions on success (v1 UX):**

- Includes capture actions into reasoning (copy/paste-ready):
  - `think op=call cmd=think.idea.branch.create` (personal lane)
  - `think op=call cmd=think.idea.branch.create` with `agent_id=null` (shared lane)
- The capture action's `args.meta.step` is populated when the workspace focus is a TASK with
  an open step (step-aware grafting).

## docs.export

Export artifacts (legacy `export`). Use `system op=schema.get` for the exact args schema.
