# Contracts — Delegation (Jobs) (v1.6)

> ✅ **v1 portal naming:** v1 exposes only **10 tools**. Delegation uses the `jobs` portal
> (`op="call"` + `cmd="jobs.*"`). Legacy tool names like `tasks_jobs_*` are **rejected**
> (`UNKNOWN_TOOL`). Task views referenced here map as:
> - `tasks_snapshot` → `tasks` + `cmd="tasks.snapshot"`
> - `tasks_macro_delegate` → `tasks` + `cmd="tasks.macro.delegate"`
> See `V1_OVERVIEW.md` + `V1_MIGRATION.md`.

Goal: allow AI agents to **delegate complex work** and track progress/results **without losing narrative**,
while keeping daily views low-noise under tight budgets.

## Safety boundary (non-negotiable)

BranchMind is deterministic and does **not** execute arbitrary external programs.

Delegation is modeled as a **job protocol** inside the store:

- BranchMind creates and tracks `JOB-*` entities.
- Work is executed out-of-process by a **runner** (`bm_runner`) that polls/claims jobs and
  reports progress/results back via MCP tools.

Flagship DX option:

- `bm_mcp` may auto-start the first-party `bm_runner` binary when jobs are queued and no runner
  lease is active. This is limited to `bm_runner` only (no arbitrary program execution) and can
  be controlled via `--runner-autostart` / `--no-runner-autostart` and `BRANCHMIND_RUNNER_AUTOSTART=1|0`.
  - Default: enabled in `daily|core`, disabled in `full`.

This keeps the server safe and deterministic while still enabling real execution.

## What a “job” represents

A job is a durable record of delegated work:

- stable id (`JOB-*`),
- linkage to meaning (`anchor_id`) and/or execution (`task_id`),
- a runner-facing spec (prompt + constraints + expected outputs),
- a status lifecycle (QUEUED → RUNNING → DONE/FAILED/CANCELED),
- a bounded event log (progress updates).

Jobs are **logistics**, not “knowledge”:

- The actual research results should land as **cards/evidence/tests** linked to anchors/tasks.
- Job completion should reference those artifacts via stable refs (`CARD-*`, `notes_doc@seq`, `TASK-*`).

## Multi-executor routing (v1)

Jobs may request a specific executor or defer to policy-based auto routing.

### Job meta (v1)

- `executor`: `codex | claude_code | auto`
- `executor_profile`: `fast | deep | audit`
- `executor_model` (optional, for `executor=claude_code`): model selector string passed to the CLI (e.g. `sonnet`/`opus`)
- `policy` (for `executor=auto`):
  - `prefer`: ordered list of executors
  - `forbid`: excluded executors
  - `min_profile`: minimal acceptable profile
- `expected_artifacts`: `report | diff | patch | bench | docs_update`

#### Copy/paste: create a Claude Code job

This pins the job to the `claude_code` executor and a specific model. The runner will pick it up when
`claude` is available on PATH (auto-detected) or when explicitly configured via `--claude-bin` / `BM_CLAUDE_BIN`.

```json
{
  "workspace": "<workspace>",
  "op": "create",
  "args": {
    "title": "Investigate <topic>",
    "prompt": "<what to do>",
    "kind": "research",
    "priority": "normal",
    "task": "TASK-123",
    "anchor": "a:core",
    "executor": "claude_code",
    "executor_profile": "fast",
    "expected_artifacts": ["report"],
    "meta": {
      "executor_model": "sonnet",
      "skill_profile": "research"
    }
  }
}
```

Tip: `executor`/`executor_profile` are first-class args and are also persisted into `job.meta` for
introspection and deterministic auto routing.

#### Copy/paste: create an auto-routed job (prefer Claude Code)

This keeps the intent high-level (`executor=auto`) but nudges routing deterministically via policy.
If a `claude_code` runner is available it will be selected first; otherwise the job falls back to `codex`.

```json
{
  "workspace": "<workspace>",
  "op": "create",
  "args": {
    "title": "Implement <feature>",
    "prompt": "<what to do>",
    "kind": "codex_cli",
    "priority": "high",
    "task": "TASK-123",
    "anchor": "a:core",
    "executor": "auto",
    "executor_profile": "deep",
    "expected_artifacts": ["patch"],
    "policy": {
      "prefer": ["claude_code", "codex"],
      "forbid": [],
      "min_profile": "fast"
    },
    "meta": {
      "executor_model": "opus",
      "skill_profile": "strict"
    }
  }
}
```

### Runner capabilities

Runners advertise:

- `runner_id`
- `executors` + supported `profiles`
- `max_parallel`
- `supports_artifacts`
- `sandbox_policy`

First-party runner (`bm_runner`) executors:

- `codex` is always available (configurable via `--codex-bin` / `BM_CODEX_BIN`).
- `claude_code` is available when the `claude` CLI is on PATH (**auto-detected**) or when configured via
  `--claude-bin` / `BM_CLAUDE_BIN` (uses the official Claude Code CLI in headless mode with structured JSON output).

### Deterministic routing

`auto` selection is deterministic:

1. filter by capabilities + `forbid` + `min_profile` + `expected_artifacts`
2. rank by `prefer` + availability + queue length
3. tie-break by `runner_id` lexicographic

## Noise control (no agent_id dependency)

Jobs must not rely on `agent_id` for visibility.

Default UX:

- `tasks_snapshot` shows at most **one** active job line for the focused task (if any).
- `tasks_jobs_open` shows a bounded, most-recent-first event log.
- `tasks_jobs_radar` lists active jobs with “attention” flags (cheap supervision).
- `tasks_jobs_tail` follows job events incrementally (no “lose place” loops).
- Deep history is available via `limit`/`cursor` knobs, never dumped by default.

## Teamlead inbox: `tasks_jobs_radar fmt=lines` (BM-L1) (v1.6)

`tasks_jobs_radar` is the **manager inbox** for delegated work.

### Multi-terminal dogfood (operator recipe)

Goal: verify the “glance → one action” loop works even with multiple terminals/runners.

1) In a manager terminal:
   - create a task (`tasks_macro_start`) and delegate 2 jobs (`tasks_macro_delegate`).
2) In runner terminal A:
   - start `bm_runner` with an explicit `--runner-id` (e.g. `r1`), and the workspace.
3) In runner terminal B:
   - start `bm_runner` with a different `--runner-id` (e.g. `r2`).
4) Back in the manager terminal:
   - call `tasks_jobs_radar fmt=lines` and confirm:
     - header shows `runner=<offline|idle|live>` (explicit, lease-based),
     - header shows `runners=<none|live:n idle:n offline:n>`,
     - each job line starts with `last.ref`, includes `| open id=<last.ref>`,
     - any `?` row includes a copy/paste `reply reply_job=...`.

Pass criteria: you can understand “who is alive and doing what” without opening anything, and when
attention is needed you can resolve it with exactly one copy/paste command.

Design constraints:

- It must survive `BUDGET_TRUNCATED` (navigation is still possible).
- Each job row must be **ref-first**: a stable pointer is the first token on the line.
- Each job row must include exactly one **copy/paste next move**:
  - always: open the latest event ref,
  - if attention is needed: show a reply hint (manager → agent).

### Attention markers (single char)

- `!` — attention: either error (`has_error=true`) or proof gate (`needs_proof=true`)
- `?` — needs manager decision (agent asked a question after last manager reply)
- `~` — stale (RUNNING but claim lease expired: `claim_expires_at_ms <= now_ms`)

### Line format (normative)

Header line (first line):

```
<workspace?> jobs_radar count=<n> runner=<offline|idle|live> runners=<none|live:<n> idle:<n> offline:<n>> status=<...?> task=<...?> anchor=<...?> stale_after_s=<n?> has_more=<bool?>
```

Optional bootstrap line (shown when there are queued jobs and runner is offline):

```
CMD: <copy/paste runner start command>
```

Optional runner lines (multi-runner diagnostics, bounded):

```
runner <idle|live> <runner_id> | open id=runner:<runner_id> job=<JOB-*>? | open id=<JOB-*>?
```

Optional offline runner lines (recently expired runner leases, bounded) (v1.9):

```
runner offline <runner_id> last=<idle|live>? last_job=<JOB-*>? | open id=runner:<runner_id> | open id=<JOB-*>?
```

Notes:

- Offline runner lines are **facts** derived from expired runner leases (`lease_expires_at_ms <= now_ms`), not heuristics.
- The list is bounded to stay glanceable under tight budgets.

Optional runner conflict lines (bounded, no-heuristics):

```
! diag <kind> runner=<runner_id>? job=<JOB-*>? | open id=runner:<runner_id>? | open id=<JOB-*>?
~ diag <kind> runner=<runner_id>? job=<JOB-*>? | open id=runner:<runner_id>? | open id=<JOB-*>?
```

Job line (one per job):

```
<last.ref> <marker?> <job_id> (<status>) <title> | open id=<last.ref> | reply reply_job=<job_id> reply_message="..."?
```

Notes:

- `<last.ref>` is always expected to exist because job creation emits a `created` event.
- `reply reply_job=... reply_message="..."` is shown only when marker is `?` (needs_manager=true).
- The row may optionally include a short `last` preview, but it must not be required for navigation.

## Runner liveness (explicit, no-heuristics) (v1.6)

Problem: managers should never have to guess whether the runner is running.

Solution: runner liveness is represented as an explicit **lease** stored in BranchMind’s store.

- A runner periodically renews its lease via `tasks_runner_heartbeat`.
- If no active lease exists, the runner is **offline**.
- If an active lease exists and the runner reports `status=idle`, the runner is **idle**.
- If an active lease exists and the runner reports `status=live`, the runner is **live**.

This is **not inferred** from job events (no heuristics), and it survives multi-terminal usage
because it is stored in the shared store.

### `tasks_runner_heartbeat` (normative)

Inputs:

- `workspace` (required)
- `runner_id` (required) — stable id for the runner process (e.g. `bm_runner:<pid>`)
- `status` (required) — `idle|live` (runner-owned state)
- `active_job_id` (optional) — current `JOB-*` when `status=live`
- `lease_ttl_ms` (optional) — requested lease TTL; server clamps to a safe range

Semantics:

- The server stores/updates a runner lease (`lease_expires_at_ms = now_ms + lease_ttl_ms`).
- Inbox tools (`tasks_jobs_radar`) compute `offline|idle|live` from **active leases only**.

## Quality control (principal workflow)

Quality is enforced via task discipline, not by job magic:

- Delegation macros should create tasks with a clear DoD and verification step(s).
- Strict reasoning mode + proof capture is the default for “principal” delegated work.
- Job completion should attach: what changed, what was tested, and where the evidence is stored.

## Skill injection (runner → subagent) (v1.8)

Goal: make delegated agents behave consistently (“BranchMind-native”) across terminals/sessions
without turning delegation into bureaucracy.

Mechanism: the external runner (e.g. `bm_runner`) may call the BranchMind `skill` tool and inject
the returned behavior pack into the subagent prompt.

Normative requirements:

- Skill packs are **bounded** (`max_chars`) and **truncation-safe**.
- Injection is **deterministic** (stable selection rules; no randomness).
- Injection is **non-fatal**: if the `skill` tool is unavailable, the runner proceeds without it.

Selection rules (deterministic):

1) If `job.meta.skill_profile` is present and valid (`daily|strict|research|teamlead`), use it.
2) Else, infer from `job.kind` when applicable (e.g. `kind` contains `research` ⇒ `research`).
3) Else, use the runner default (recommended: `strict`).

Budget rules:

- If `job.meta.skill_max_chars` is present and numeric:
  - `0` disables injection for that job,
  - otherwise it overrides the runner default (clamped to a safe upper bound).

## Runner protocol (high level)

1) Create job (usually via `tasks_macro_delegate` or `tasks_jobs_create`).
2) Runner calls `tasks_jobs_list` (status=QUEUED), picks a job.
3) Runner calls `tasks_jobs_claim` (transition QUEUED → RUNNING; receives `job.revision` claim token and `claim_expires_at_ms`).
4) Runner executes the work out-of-process.
5) Runner periodically calls `tasks_jobs_report` (bounded progress + renew claim lease; includes `runner_id` + `claim_revision`).
6) Runner calls `tasks_jobs_complete` (includes `runner_id` + `claim_revision`) with:
   - final status (`DONE`/`FAILED`),
   - short summary,
   - stable refs to artifacts (cards/notes/tasks) that contain the real knowledge.

If a runner crashes, jobs remain durable and can be reclaimed after a lease timeout:

- runners use `tasks_jobs_report` heartbeats to renew the **job claim lease** (time-slice),
- a new runner may reclaim an expired `RUNNING` job via `tasks_jobs_claim allow_stale=true`.

This enables multi-hour (up to 24h) delegated work without “stuck forever” jobs.

## Job claim lease (time-slices, reclaim) (explicit, no-heuristics) (v1.7)

Problem: a long-running job must survive restarts and time, but must also be reclaimable without
double-execution and without manager “guesswork”.

Solution: each `RUNNING` job has an explicit **claim lease** (time-slice) stored in the job row:

- On claim/reclaim, the server sets `claim_expires_at_ms = now_ms + lease_ttl_ms`.
- On each runner report/heartbeat, the server extends the lease the same way.
- If `claim_expires_at_ms <= now_ms`, the claim is **expired** and the job is reclaimable.

No heuristics: lease expiry is the only default reclaim precondition.

### Claim token (prevents “zombie runner” writes)

The job’s `revision` is a claim token:

- `tasks_jobs_claim` increments `job.revision` and returns it to the runner.
- `tasks_jobs_report` and `tasks_jobs_complete` must include `claim_revision` (and `runner_id`).
- If the current job row does not match that `(runner_id, claim_revision)`, the server rejects the write.

This prevents old/stale runners from continuing to write events or completing a job after it has been reclaimed.

### Claim/reclaim event meta (reclaim reasons) (v1.8)

On successful claim/reclaim, the server emits a job event (`kind="claimed"` or `kind="reclaimed"`).

For `kind="reclaimed"`, the event may include a small `meta` object:

```json
{
  "previous_runner_id": "string?",
  "reason": "ttl_expired|manual|conflict_resolved"
}
```

Semantics:

- `previous_runner_id` is the runner that held the prior claim (if known).
- `reason` is deterministic and explains *why* the reclaim happened.
  - `ttl_expired`: default reclaim via `allow_stale=true` once `claim_expires_at_ms <= now_ms`.
  - `manual`: explicit operator action (if/when implemented).
  - `conflict_resolved`: reclaim performed as part of resolving a multi-runner conflict (if/when implemented).

## Feedback loop (agent → manager) (v1)

Delegated agents should provide **structured, low-noise feedback** during execution:

- Prefer *durable artifacts* (cards/evidence/tests) for real findings.
- Use job events for *navigation and supervision* (short updates + stable refs).

Recommended event kinds:

- `checkpoint`: “a meaningful milestone was reached” (what changed + what was verified).
- `progress`: “still working” (short, actionable).
- `question`: “I need an explicit decision to proceed” (must be manager-visible).
- `heartbeat`: “alive” (runner liveness; coalesced to avoid spam).

Two compatible mechanisms are supported:

1) Direct event calls: the delegated agent (Codex session) calls `tasks_jobs_report` itself using `workspace` + `job` given in the job prompt.
2) Runner-mapped events: the delegated agent returns a bounded `events[]` list in its structured output; the runner translates them into `tasks_jobs_report` calls.

This makes supervision cheap: the manager can open `JOB-*` and jump to the referenced artifacts (`CARD-*`, `notes@seq`, `TASK-*`).

## Skill pack injection (runner → subagent) (v1)

Problem: delegated subagents should behave consistently across sessions and terminals without relying
on “tribal knowledge”.

Solution: runners may inject a bounded behavior pack from the `skill` tool into the subagent prompt.

Optional job meta fields (stored on job creation):

- `skill_profile`: `daily|strict|research|teamlead` (runner may default to `strict`)
- `skill_max_chars`: integer output budget for the injected pack
  - `0` disables injection for this job

This keeps delegation **AI-native** (same loop everywhere) while preserving determinism and bounded
outputs.

## Feedback loop (manager → agent) (v1)

Managers must be able to steer a long-running delegated job without restarting it.

- Use `tasks_jobs_message` to send a short instruction/answer to a `QUEUED` or `RUNNING` job.
- The runner should inject a **bounded job thread** (recent non-heartbeat events, including manager messages and agent questions) into the next execution slice prompt.
- Keep messages short and point to durable context via stable refs (`CARD-*`, `notes@seq`, `TASK-*`, `a:*`).

### Clearing `needs_manager` (normative)

The manager response mechanism must be **hunt-free**:

- the agent raises attention via `tasks_jobs_report kind=\"question\"`,
- the manager clears attention via either:
  - `tasks_jobs_message` (low-level tool), or
  - `tasks_jobs_radar` reply shortcut (`reply_job` + `reply_message`),
  both stored as a `manager` event kind,
- `needs_manager` is computed deterministically as:
  - `last_question_seq > last_manager_seq` AND status in `{RUNNING, QUEUED}`.

## Fan-out / fan-in macros (v1.5)

Large tasks must be splittable and mergable without ad-hoc spreadsheets.

### `tasks_macro_fanout_jobs`

Create multiple jobs (3–10 recommended) from a single intent, typically split by anchors.

Inputs (high level):

- `workspace`
- `task` (optional; defaults to focus)
- `anchors[]` (required; anchor ids)
- `title_prefix` (optional)
- `prompt` (required; base prompt)
- job knobs: `job_kind`, `job_priority`

Semantics:

- Deterministic ordering: anchors are processed in `id` ascending order.
- For each anchor:
  - create one `JOB-*` with `anchor=<id>` and `task=<task>` (if provided),
  - prompt is derived deterministically as: `prompt + \"\\n\\nAnchor: <id>\"`.
- Output returns: created job ids + their first refs (created event) for navigation.

### `tasks_macro_merge_report`

Merge delegated work back into a **single canonical report** that is fast to resume.

Inputs (high level):

- `workspace`
- `task` (optional; defaults to focus)
- `jobs[]` (required)
- `title` (optional)
- `pin` (optional; default true)

Semantics:

- Reads each job summary and completion refs (bounded).
- Produces a deterministic, diff-friendly report:
  - what changed,
  - what was verified (proof receipts),
  - stable refs to the durable artifacts,
  - open risks / next actions.
- Stores the merge report as a pinned canonical artifact (so it survives restarts).

## Quality by default: proof gate (runner policy) (v1.6)

Delegated work must not “complete” without evidence.

Policy:

- A runner MUST treat `DONE` without stable `refs[]` as non-final.
- If the agent returns `DONE` but `refs[]` is empty (or only contains placeholders), the runner converts it to `CONTINUE` and asks for the missing proof:
  - at least one stable `ref` (`CARD-*`, `TASK-*`, `notes_doc@seq`, `JOB-*`),
  - and at least one proof receipt line (`CMD:` and/or `LINK:`) when applicable.
- Server-side DX: when `tasks_jobs_complete` is called with `status=DONE` and `refs[]` is empty, but `summary` contains proof-like pointers, the server may salvage bounded `refs[]` deterministically:
  - receipts (`CMD:`/`LINK:`) and strong shell-like commands,
  - embedded stable ids (`CARD-*`, `TASK-*`, `PLAN-*`, `JOB-*`, `notes@*`, `a:*`).
  This reduces false “proof missing” loops while preserving determinism (no execution).
- Server-side DX (manager steering): `tasks_jobs_message` follows the same spirit — if `refs[]` is empty but the message body contains stable ids/receipts, the server may salvage bounded `refs[]` deterministically (metadata extraction only).
- Proof satisfaction (DX): a manager message with at least one stable ref counts as “proof satisfied” for inbox purposes:
  - `needs_proof` is cleared when such a message happens after the last `proof_gate` event,
  - this avoids forcing an extra runner checkpoint when the manager already provided the missing evidence pointer.
- The runner should surface this as an explicit inbox signal via `tasks_jobs_report kind=proof_gate` (attention-worthy, but not `needs_manager`).

This preserves server determinism while making “DONE means done” reliable.

## Long-running jobs (24h loop)

For long investigations, runners should treat a job as a **loop**, not a single monolith:

- send periodic heartbeats (`kind=heartbeat`) so users can distinguish “alive” from “stuck”,
- optionally time-slice execution (e.g., 20–60 minutes per slice) to reduce the risk of provider/session timeouts,
- ensure the delegated agent lands intermediate results as durable artifacts (cards/evidence) linked to anchors/tasks,
  so progress is not lost if a slice crashes.

## Reference runner (bm_runner) (v0)

This repository ships a small external runner binary, `bm_runner`, to make delegation “real”:

- It polls `JOB-*` records, claims them, and reports progress/results back via MCP tools.
- It can execute a headless Codex session (`codex exec`) and require a strict JSON output schema.
- It keeps progress low-noise and completion refs jump-friendly (`CARD-*`, `JOB-*`, `TASK-*`, `notes@seq`, ...).

Operational note:

- The server (`bm_mcp`) remains deterministic: `bm_runner` is the only component that executes anything.

## Multi‑executor delegation (v1)

Jobs can target multiple executors; routing is deterministic.

### Job meta

```json
{
  "executor": "codex|claude_code|auto",
  "executor_profile": "fast|deep|audit",
  "policy": {
    "prefer": ["claude_code", "codex"],
    "forbid": [],
    "min_profile": "audit"
  }
}
```

### Runner capabilities

Runners advertise:

- `runner_id`
- `executors: ["codex", "claude_code"]`
- supported profiles per executor
- `max_parallel`
- `sandbox_policy`

### Deterministic auto routing

1. Filter by `capabilities` + `forbid` + `min_profile`.
2. Rank by `prefer` then availability/queue length.
3. Stable tie‑break: `runner_id` lexicographic.

Routing decisions are pure + deterministic; tests lock behavior.
