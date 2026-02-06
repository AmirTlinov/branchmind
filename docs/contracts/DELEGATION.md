# Contracts — Delegation (Jobs) (v1)

BranchMind delegation is exposed via the **`jobs` portal tool**.

- Tool: `jobs`
- Operations: `op="call" + cmd="jobs.*"` (schema-discoverable via `system` → `schema.get`)
- Optional convenience: `op="<alias>"` for a small set of golden ops (discoverable via `tools/list`)

Goal: allow AI agents to **delegate complex work** and track progress/results **without losing narrative**,
while keeping daily views low-noise under tight budgets.

---

## Safety boundary (non‑negotiable)

BranchMind is deterministic and does **not** execute arbitrary external programs.

Delegation is modeled as a durable **job protocol** inside the embedded store:

- BranchMind creates and tracks `JOB-*` entities.
- Work is executed out-of-process by a **runner** (`bm_runner`) that polls/claims jobs and
  reports progress/results back via MCP tools.

Optional DX:

- `bm_mcp` may auto-start the first-party `bm_runner` binary when jobs are queued and no runner
  lease is active. This is limited to `bm_runner` only (no arbitrary program execution) and can
  be controlled via `--runner-autostart` / `--no-runner-autostart` and `BRANCHMIND_RUNNER_AUTOSTART=1|0`.

This keeps the server safe and deterministic while still enabling real execution.

---

## What a “job” represents

A job is a durable record of delegated work:

- stable id (`JOB-*`),
- linkage to meaning (`anchor_id`) and/or execution (`task_id`),
- a runner-facing spec (prompt + constraints + expected outputs),
- a status lifecycle (`QUEUED → RUNNING → DONE|FAILED|CANCELED`),
- a bounded event log (progress updates).

Jobs are **logistics**, not “knowledge”:

- the real findings should land as **cards/evidence/tests** linked to anchors/tasks,
- job completion should reference those artifacts via stable refs (`CARD-*`, `notes_doc@seq`, `TASK-*`).

---

## Job routing meta (v1)

Jobs may request a specific executor or defer to deterministic auto routing.

Job fields (high level):

- `executor`: `codex | claude_code | auto`
- `executor_profile`: `fast | deep | audit`
- `policy` (for `executor=auto`):
  - `prefer`: ordered list of executors
  - `forbid`: excluded executors
  - `min_profile`: minimal acceptable profile
- `expected_artifacts`: `report | diff | patch | bench | docs_update`
- `meta`: executor-specific knobs (e.g. `executor_model`, `skill_profile`)

### Copy/paste: create a Claude Code job (v1)

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
      "skill_profile": "deep"
    }
  }
}
```

### Copy/paste: create an auto‑routed job (prefer Claude Code)

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

---

## Core job commands (semantic index)

Use `system` → `schema.get(cmd)` for exact schemas and examples.

Common flows:

- `jobs.create` — create a job (does not execute anything)
- `jobs.list` — list jobs (bounded)
- `jobs.open` — open one job (status + prompt + recent events; bounded)
- `jobs.tail` — follow events incrementally (cursor/after_seq)
- `jobs.radar` — manager inbox (glanceable supervision, ref‑first)
- `jobs.claim` — claim/reclaim a job slice (`allow_stale=true` for reclaim once the lease expires)
- `jobs.report` — progress update + renew claim lease
- `jobs.complete` — final completion (`DONE|FAILED`)
- `jobs.message` — manager steering (answer a question / adjust constraints)
- `jobs.cancel` — cancel a queued job (safe boundary)
- `jobs.wait` — bounded polling helper
- `jobs.runner.heartbeat` — runner liveness lease (explicit, no heuristics)
- `jobs.runner.start` — optional explicit runner bootstrap helper

---

## Noise control (no agent_id dependency)

Jobs must not rely on `agent_id` for visibility.

Default UX:

- `tasks.snapshot` shows at most **one** active job line for the focused task (if any).
- Deep history is available via `limit`/`cursor` knobs; nothing is dumped by default.

---

## Teamlead inbox: `jobs.radar fmt=lines` (BM‑L1)

`jobs.radar` is the **manager inbox** for delegated work.

Pass criteria: you can understand “who is alive and doing what” without opening anything, and when
attention is needed you can resolve it with exactly one copy/paste command.

Design constraints:

- It must survive `BUDGET_TRUNCATED` (navigation is still possible).
- Each job row must be **ref-first**: a stable pointer is the first token on the line.
- Each job row must include exactly one **copy/paste next move**:
  - always: open the latest event ref,
  - if attention is needed: show a reply hint (manager → agent).

Attention markers (single char):

- `!` — attention: either error (`has_error=true`) or proof gate (`needs_proof=true`)
- `?` — needs manager decision (agent asked a question after last manager reply)
- `~` — stale (RUNNING but claim lease expired: `claim_expires_at_ms <= now_ms`)

Line format (normative):

Header line (first line):

```
<workspace?> jobs_radar count=<n> runner=<offline|idle|live> runners=<none|live:<n> idle:<n> offline:<n>> status=<...?> task=<...?> anchor=<...?> stale_after_s=<n?> has_more=<bool?>
```

Job row:

```
<last.ref> <marker?> <job_id> (<status>) <title> | open id=<last.ref> | reply reply_job=<job_id> reply_message="..."?
```

Notes:

- `<last.ref>` is always expected to exist because job creation emits a `created` event.

---

## Runner liveness (explicit lease, no heuristics)

Managers should never have to guess whether the runner is running.

Solution: runner liveness is represented as an explicit **lease** stored in the shared store.

- A runner periodically renews its lease via `jobs.runner.heartbeat`.
- If no active lease exists, the runner is **offline**.
- If an active lease exists and the runner reports `status=idle`, the runner is **idle**.
- If an active lease exists and the runner reports `status=live`, the runner is **live**.

This is **not inferred** from job events (no heuristics), and it survives multi-terminal usage.

---

## Job claim lease (time‑slices, reclaim) (explicit, no heuristics)

Problem: a long-running job must survive restarts and time, but must also be reclaimable without
double-execution and without manager “guesswork”.

Solution: each `RUNNING` job has an explicit **claim lease** (time-slice) stored in the job row:

- On claim/reclaim, the server sets `claim_expires_at_ms = now_ms + lease_ttl_ms`.
- On each runner report/heartbeat, the server extends the lease the same way.
- If `claim_expires_at_ms <= now_ms`, the claim is **expired** and the job is reclaimable.

Claim token (prevents “zombie runner” writes):

- `jobs.claim` increments `job.revision` and returns it to the runner.
- `jobs.report` and `jobs.complete` must include `claim_revision` (and `runner_id`).
- If the current job row does not match that `(runner_id, claim_revision)`, the server rejects the write.

---

## Proof gate (runner policy)

Delegated work must not “complete” without evidence.

Policy:

- A runner MUST treat `DONE` without stable `refs[]` as non-final.
- If the agent returns `DONE` but `refs[]` is empty, the runner converts it to `CONTINUE` and asks for the missing proof:
  - at least one stable ref (`CARD-*`, `TASK-*`, `notes_doc@seq`, `JOB-*`),
  - and at least one proof receipt line (`CMD:` and/or `LINK:`) when applicable.

This preserves server determinism while making “DONE means done” reliable.

---

## Fan‑out / fan‑in macros (tasks)

Large tasks must be splittable and mergable without ad-hoc spreadsheets.

Canonical macros (task domain):

- `tasks.macro.fanout.jobs` — create multiple jobs (3–10 recommended) from a single intent (typically split by anchors).
- `tasks.macro.merge.report` — merge delegated work back into a single canonical report that is fast to resume.

Implementation detail: these are task commands (tool=`tasks`, `op="call"`, `cmd="tasks.macro.*"`),
not job commands.
