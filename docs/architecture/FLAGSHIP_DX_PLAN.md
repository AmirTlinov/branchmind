# Flagship AI-Native DX Plan (BranchMind)

This is a **productization plan** for turning BranchMind into a *flagship* AI-native development OS:
agents resume by meaning in seconds, delegation scales to many long-running subagents, and managers
stay in control with low-noise, deterministic outputs.

> ✅ **v1 portal naming:** This repo has cut over to **10 tools**. When this doc mentions legacy
> tool names (e.g. `tasks_snapshot`, `think_card`), read them as `tasks`/`think` portal calls with
> `op="call"` + `cmd="tasks.snapshot"` / `cmd="think.card"`. Legacy tool names are rejected.

This document is intentionally **detailed** and **actionable**:
- contract-first (docs/contracts are the spec),
- reversible (feature flags / soft defaults),
- evidence-first (tests/proofs, not vibes),
- low-noise defaults (progressive disclosure).

---

## 0) North Star

### 0.1 Product promise (what “flagship” means)

**After `/compact` or a new session:**
- Resume in **<30 seconds** by reading *meaning*, not code.
- See exactly **one focus** (or at most 3) and a copy/paste **next action** + **backup**.
- Everything else exists, but does **not** distract unless explicitly requested.

**For delegation:**
- One manager can run **3–10 jobs** in parallel (or more) using an inbox.
- Subagents can run **hours → 24h**, with explicit liveness and reclaim behavior.
- `DONE` is trusted: it carries **proof refs** (or an explicit override with `reason+risk`).

### 0.1.1 Operational UX target (what the manager actually does)

In the “teamlead” loop, the manager repeatedly does only these actions:

1) Read `tasks_snapshot` (BM‑L1) — get the compass.
2) Read `jobs_radar` — see which subagent needs attention.
3) If a job shows `?`, open it, answer once, job resumes.
4) If a job shows `!` (proof missing / blocked), open it, request proof or provide override.
5) If a job is `offline`, requeue/reclaim (one action).
6) When enough jobs are `✓`, run fan‑in merge report (one action).

Success criterion: the manager never needs to “search chat logs” to find what happened.

### 0.2 Hard constraints (non-negotiable)

- **Deterministic outputs** (stable ordering, no random IDs in payloads).
- **Budget-safe** reads everywhere (bounded output, predictable truncation).
- **No “chat hunting”**: every actionable item has a `ref` and a 1-step action.
- **No data loss**: we hide via horizons/visibility; we don’t delete history by default.

### 0.3 “Single organism” invariant

All execution events (tasks/jobs/runners) must be **durably evented** into the reasoning subsystem
atomically, so the narrative is *the system*, not the chat transcript.

### 0.4 Quality metrics (product SLOs)

We treat “quality” as measurable defaults, not taste.

**Resume SLOs (new session / after `/compact`):**
- `median_resume_time_s < 30` (time from first snapshot → first concrete action).
- `snapshot_actions_count <= 2` (primary + backup; no action sprawl).
- `snapshot_has_ref == true` under `max_chars <= 2000` (truncation safety).

**Noise SLOs:**
- `active_horizon_count <= 3` by default (warn if higher).
- `backlog_is_hidden_by_default == true` (only counts shown).
- `draft_hidden_outside_focus == true` (unless explicitly requested).

**Reinvention SLOs:**
- `duplicate_decisions_rate` trends down over time (measured via merges/supersedes).
- `time_to_find_prior_decision_s < 15` given an anchor id.

**Delegation SLOs:**
- `jobs_done_without_manager_touch_rate` trends up (healthy autonomy).
- `false_offline_rate` near zero (runner liveness trust).
- `done_without_proof_rate == 0` (unless explicit override recorded).

### 0.5 Guarantee classes (G1–G4)

These are the **product guarantees** that keep BranchMind a “second brain” instead of a giant log.
Each guarantee must have: mechanism, UX behavior, contract shape locks, and tests/metrics.

**G1 — Anti‑kasha (navigability by default)**  
Guarantee: default views show **one focus**, **one next** (+ optional backup), and **counts not lists**.
Everything else is hidden but addressable by `ref`.

Mechanisms:
- horizons + visibility defaults (active vs backlog vs parked),
- BM‑L1 truncation invariant (first lines survive),
- `active requires next action` (otherwise demote/park).

Evidence: `snapshot_actions_count <= 2`, `snapshot_has_ref == true` under tight budgets.

**G2 — Context is never lost, and never “mixes itself”**  
Guarantee: any meaningful progress exists as a durable artifact with a stable `ref`, scoped to meaning
(anchors) and discoverable without scanning chat logs.

Mechanisms:
- single‑organism invariant (execution → durable reasoning events),
- anchor binding for work + reasoning (`task/job ↔ anchor ↔ decision/evidence`),
- canon lifecycle (draft → canon → superseded; no “rewrite history”).

Evidence: `time_to_find_prior_decision_s < 15` given an anchor; reinvention rate trends down.

**G3 — Principal‑grade planning & decomposition by default**  
Guarantee: active work is decomposed into verifiable, bounded steps with DoD + proof plan, and bad
plans become **refactorable objects** (lint → one‑command patch), not dead ends.

Mechanisms:
- Planning Quality Layer (lint taxonomy + patch suggestions),
- Principal Plan Standard (Goal/DoD/Next/Proof/Risks/Anchors),
- hard gate only for `DONE without proof` (override allowed with `reason+risk`).

Evidence: `done_without_proof_rate == 0` (unless override); “bad plan → incremental patches → good plan”.

**G4 — Thinking amplification without bureaucracy**  
Guarantee: BranchMind applies friction only where it pays rent (risk, proof, boundedness) and stays
low-noise otherwise; skills/playbooks nudge deeper thinking without forcing form-filling.

Mechanisms:
- skill packs (`daily|strict|deep|teamlead`) are short, versionable, truncation-safe,
- “falsifier + stop criteria” in deep mode (research accepted as an alias),
- progressive disclosure (draft/history/backlog only by explicit request).

Evidence: fewer useless CONTINUE loops; autonomy rate trends up; low false-offline rate.

---

## 1) Key concepts (the minimum vocabulary)

### 1.0 Second Brain Core (the 5 artifact types)

The core product idea is not “store everything” — it is “store **stable artifacts** and make the UI
show **pointers** (refs), not prose”.

Primary artifacts (v1):
- **Anchor**: where in meaning/architecture we are (`a:<slug>`).
- **Work item**: task/step/job/runner state machines (what we’re doing).
- **Card**: reasoning units (hypothesis/test/evidence/decision/update), with lifecycle (draft/canon/superseded).
- **Proof/Evidence**: receipts (`CMD:`/`LINK:` + refs) that make `DONE` trustworthy.
- **Pack**: semantic compaction (mindpack/cockpit) — a bounded, versioned navigation index for fast resume.

Non-negotiable UX rule: every actionable output line must be either
1) a stable `ref`, or 2) a copy/paste action that produces a stable `ref`.

### 1.1 Horizons (anti-overload layer)

We treat “too many open plans” as a *UX bug*, solved by **horizons**:

- **Active Horizon**: 1–3 active initiatives the agent may think about by default.
- **Backlog**: everything else (still queryable, but hidden by default).
- **Parked/Snoozed**: backlog item with a wake-up condition (time or explicit unpark).
- **Stale**: item that has not moved recently (computed or stored) and should be hidden by default
  unless it’s the focus, pinned, or explicitly requested.

### 1.2 Canon vs Draft (anti-contradiction layer)

We separate “what we believe” from “what we tried”:

- **Draft**: explorations, partial ideas, questions, tentative hypotheses.
- **Canon**: decisions + evidence + tests that prevent reinvention.
- **Pinned**: minimal resume surface (cockpit, top decisions, current risks).
- **Superseded**: canon that used to be true, but is no longer the winning decision.

### 1.3 Anchors (meaning map)

An **anchor** is a stable semantic ID for an architecture area (boundary/component/contract/test-surface/ops).
Anchors are the map that binds plans, notes, tests, evidence, and jobs.

### 1.4 Jobs & Runners (delegation engine)

- **Job**: a delegated unit of work; owned by a manager initiative (often a task/anchor).
- **Runner**: a worker process that claims jobs, runs slices, heartbeats, and emits events.

Key property: a manager sees “who is alive and doing what” without guessing.

### 1.5 Proof gate

“DONE” is only accepted if:
- proof refs exist (e.g. `CMD:` / `LINK:` / stable `ref` IDs), or
- explicit override exists (with `reason+risk`).

### 1.6 Research loop (scientific mode, long investigations)

Long investigations must not devolve into chat logs. The unit of progress is:

`hypothesis → minimal falsifier test → evidence → decision (canon)` (+ rollback/kill-switch)

Design requirements:
- Every hypothesis has a named risk and a falsifier test.
- Every decision links to the evidence that justifies it.
- Stop criteria are explicit (time/budget/signal) to prevent infinite loops.
- Investigation is anchor-scoped so it is retrievable without scanning the whole project.

### 1.7 Playbooks & skills (behavior shaping without bureaucracy)

We ship **built-in behavior packs** (versioned, deterministic) so agents behave consistently:
- `daily`: low-noise compass loop
- `strict`: DoD/proof discipline; override requires reason+risk
- `deep`: scientific loop + stop criteria + evidence hygiene (research accepted as an alias)
- `teamlead`: fan-out/fan-in + inbox protocol

These packs must be:
- short by default (bounded by `max_chars`),
- composable (progressive disclosure),
- injectable by runner for subagents.

---

## 1.8 State machines (explicit, to prevent drift)

### 1.8.1 Task horizon state machine

Minimal v1 states (stored):

- `active`: shown by default in snapshot/radar.
- `backlog`: hidden by default (counted, expandable).
- `parked`: backlog with a wake policy (time/manual).
- `done`: closed, never shown by default unless referenced.
- `canceled`: closed, never shown by default unless referenced.

Allowed transitions:
- `active → backlog` (demote)
- `backlog → active` (promote)
- `backlog → parked` (snooze)
- `parked → backlog|active` (wake)
- `active|backlog|parked → done|canceled` (close)

Rules:
- On `focus_set`, the focused task is forcibly visible even if not active.
- A task can be `active` only if it has a “next action” (otherwise it’s churn).

### 1.8.2 Canon state machine (decisions)

Minimal v1 semantics:
- `draft`: defaults for hypothesis/question/note/update unless pinned.
- `canon`: defaults for decision/evidence/test (plus explicit publish).
- `superseded`: canon that was replaced; hidden by default.

Supersession rule:
- A canon decision may supersede exactly one previous decision (v1).
- Tie-break is deterministic: latest by stored timestamp (then by stable id).

### 1.8.3 Job state machine

Minimal v1 job states:
- `created`
- `claimed` (leased)
- `running` (has an active slice)
- `needs_manager` (explicit question / proof missing / blocked)
- `done`
- `failed` (terminal, but reopenable)

Rules:
- `needs_manager` is never silent: it must emit a `question` event with a `ref`.
- `done` requires proof refs (or explicit override recorded as event).

### 1.8.4 Runner state machine

Runner states are derived from durable heartbeat events and TTL parameters:
- `offline`: no heartbeat within TTL
- `idle`: heartbeat within TTL, no active slice lease
- `live`: heartbeat within TTL and holding an active slice lease

No heuristics based on “recent output”; only persisted facts.

---

## 2) Deliverables map (what we ship)

We ship in **layers** so each layer is independently valuable and reversible:

0) **Second Brain Core**: stable artifacts + semantic compaction (packs/cockpit) + ref-first navigation invariants.
1) **Focus Layer**: horizons + stale/snooze + snapshot UX (anti-overload).
2) **Research Layer**: scientific loop (hypothesis→test→evidence→decision) + stop criteria.
3) **Canon Layer**: supersede + cockpit + duplicate control (anti-contradiction).
4) **Planning Quality Layer**: plan/decomposition lint + playbooks + verifiable DoD.
5) **Delegation Layer**: fan-out/fan-in + feedback protocol + inbox UX (teamlead workflow).
6) **Runner Layer**: explicit liveness + 24h reliability + multi-runner diagnostics.
7) **Budget Layer**: ref-first outputs + truncation invariants (HUD survives).
8) **Skill Layer**: built-in skill packs + runner injection for subagents.

Each layer must include:
- contract docs update,
- storage + invariants,
- MCP tool updates,
- tests (contract + integration + DX),
- rollback/kill-switch.

See also: `docs/architecture/PLANNING_QUALITY.md` (incremental refinement + plan bundles).

---

## 3) Contracts-first work (docs/contracts)

### 3.1 Add/extend contract documents

**A) Horizons**
- Define horizon states, transitions, and default visibility rules.
- Define what `tasks_snapshot` shows by default and what is opt-in.

**B) Canon/Draft**
- Define visibility semantics (`v:draft`, `v:canon`, pinned, superseded).
- Define “latest canon” selection rules and ordering.

**C) Delegation**
- Define job schema (prompt, expected outputs, budgets, refs).
- Define subagent feedback protocol (events kinds, required fields).
- Define manager acknowledgement action (one command).

**D) Runners**
- Define runner registration, heartbeat schema, state machine (`live/idle/offline`).
- Define leases/time-slices/reclaim behavior and conflict diagnostics.

**E) Proof gate**
- Define what counts as proof refs and where they must live.
- Define salvage behavior (when proofs appear in text).
- Define typed errors and recovery hints.

**F) Skills & playbooks**
- Define a schema-stable `skill` tool that returns versioned behavior packs.
- Define profiles (`daily|strict|deep|teamlead`; research accepted as an alias for deep) and injection rules for runners.
- Define truncation requirements (a skill must remain useful under small budgets).

### 3.2 Contract discipline (shape locks)

For every new/changed tool:
- Inputs/outputs schema-stable.
- Budgets: `max_chars`, `limit`, `cursor` style pagination where relevant.
- Typed errors + recovery hints.
- “Truncation invariant”: even under low budgets, output keeps:
  - focus
  - `ref`
  - next action (copy/paste)
  - the minimal status line (runner/job state).

### 3.3 Contract file structure (recommended)

To keep the surface maintainable, the contract should be split by concern:
- `TYPES.md`: shared enums/fields (horizon states, runner states, event kinds).
- `TASKS.md`: task operations + snapshots + horizons.
- `MEMORY.md`: canon/draft/superseded semantics and visibility defaults.
- `DELEGATION.md`: job tools, fan-out/fan-in, feedback protocol, proof gate semantics.
- `ANCHORS.md`: anchor types/relations and meaning map constraints.
- `INTEGRATION.md`: “single organism” invariants and event guarantees.

Every section must include:
- Inputs/outputs schema (JSON),
- Semantics (state transitions),
- Budgets/truncation behavior,
- Typed errors with recovery hints.

---

## 4) Storage layer plan (data model + indexes + invariants)

### 4.1 Horizons storage

We need a cheap way to answer:
- “What is active right now?”
- “How big is the backlog?”
- “What is stale?”
- “What is parked and when does it wake?”

Implementation options:
- **Stored horizon fields** on tasks (recommended for O(1) reads).
- Or computed horizon from tags/notes (risk: expensive and noisy).

**Recommended v1**:
- Persist horizon fields on tasks (or separate horizon table indexed by task).
- Store `last_activity_ts` per task (already derivable from events; store/cached for speed).
- Store optional `parked_until_ts`.

Data fields (example; naming TBD by schema discipline):
- `horizon_state`
- `parked_until_ts_ms?`
- `stale_after_ms?` (policy; could be workspace-level default)
- `last_activity_ts_ms` (cached)
- `priority?` (optional; only if it pays rent in DX)

Index needs:
- by `horizon_state`
- by `parked_until_ts_ms` (next wake)
- by `last_activity_ts_ms` (stale sorting)

### 4.2 Canon/Draft storage

We need:
- “Which decision supersedes which?”
- “Which items are pinned/cockpit?”

Approach:
- Keep as graph/notes semantics, but ensure we can query “latest canon” boundedly.
- Store `superseded_by` edges (or dedicated column) so selection is cheap.

### 4.3 Jobs & runners storage

We need:
- job states: created → claimed → running(slices) → done / blocked / needs_manager / failed.
- runner records: `runner_id`, `last_heartbeat_ts`, `state`, `current_job?`, `capabilities?`.

Indexes:
- list jobs by state and by recency,
- list runners by last heartbeat,
- job events by (job_id, seq).

Extra index needs (multi-runner):
- by `runner_id` for “what is this runner doing?”
- by `job.state` for inbox filtering
- by `job.needs_manager` for immediate attention

### 4.4 Invariants (must be tested)

- **Single-writer job claim**: a job has one lease holder at a time.
- **Lease expiry semantics**: reclaim only after TTL (+ grace) to avoid flapping.
- **Event ordering**: events are monotonically increasing by `seq`.
- **No silent stall**: missing proof / needs_manager is represented as an explicit event.

---

## 5) MCP UX plan (what users see; “no hunting”)

### 5.1 Snapshot = the daily compass (BM‑L1)

Default `tasks_snapshot` must stay short and stable:

**Always include:**
- focus id + title
- `where=<top anchor>` (or unknown)
- `ref=<best openable ref>`
- `next=<one best copy/paste action>`
- `backup=<one backup action>` (optional but preferred)

**Never include by default:**
- long lists of plans,
- full backlog,
- large history traces.

Instead: print counts + one command to expand.

#### 5.1.1 Line protocol shape (example)

We keep outputs copy/paste-friendly and stable (exact wording TBD):

- `focus TASK-123 — <title> | where=a:<slug> | ref=CARD-999 | horizon active=1 backlog=42 parked=7 stale=9 | next <cmd> | backup <cmd>`
- `inbox jobs: need_manager=2 running=1 done_recent=3`
- `runners: live=1 idle=0 offline=1`

Truncation rule:
- if budget is tight, the **first line must survive** even if the rest truncates.

### 5.2 Horizons UX (“plan pile” solution)

Add to snapshot:
- `active_count`
- `backlog_count`
- `stale_count`
- `parked_count`
- optional `next_wake` (bounded, 1 item max)

Provide one-line actions:
- “show backlog”
- “park/snooze”
- “promote to active”
- “mark stale resolved” (or “archive to backlog”)

#### 5.2.1 Horizon lint (anti-overload policy)

Add a lint view/tool (or diagnostics section):
- `active_count > 3` ⇒ warn, suggest demotion.
- `no next action` for active tasks ⇒ warn, suggest park.
- `stale_count high` ⇒ suggest prune/merge.

This is not a hard block by default; it’s a manager/agent nudge.

### 5.3 Canon/Draft UX (“no self-contradiction”)

In smart view:
- show pinned cockpit,
- show latest canon decisions + evidence refs,
- hide superseded canon unless explicitly requested,
- hide drafts unless step-scoped or requested.

One-line actions:
- “publish decision to canon”
- “supersede previous decision”
- “show drafts for this anchor”

#### 5.3.1 Duplicate control (“don’t reinvent 392 times”)

We need an explicit strategy for duplicates:
- A canonical decision can be merged with a prior canonical decision if it is equivalent.
- A superseded decision remains retrievable by ref, but hidden from smart views.
- A “merge report” must mention which decisions were superseded/merged (so narrative stays coherent).

### 5.4 Delegation UX (teamlead inbox)

**Inbox (`jobs_radar`) becomes manager’s cockpit**:

Each row must show (bounded):
- job id
- state marker: `?` question / `!` blocked/proof-missing / `~` progress / `✓` done
- runner: `runner_id` (or `none`)
- liveness: `live/idle/offline` (explicit)
- last event summary (short)
- `last.ref` (openable `JOB-* @seq` or equivalent)
- one action hint: “open/answer/requeue”

#### 5.4.1 Inbox line format (example)

One job row should encode at most:
- marker + job id + title
- runner id + runner state
- last event short text
- `last.ref`
- one action hint

Example conceptually:
- `? JOB-042 <title> runner=r1 live last="needs decision" ref=JOB-042@19 action=open+answer`
- `~ JOB-043 <title> runner=r2 live last="80%: implementing" ref=JOB-043@12 action=open`
- `! JOB-044 <title> runner=none offline last="proof missing" ref=JOB-044@7 action=open+request-proof`

### 5.4.2 Manager “one message” protocol (ack/answer)

We want a single manager action to:
- attach answer/decision text,
- optionally attach refs (proof, spec links),
- clear `needs_manager`,
- optionally set the next best step for the job.

### 5.5 Feedback protocol UX (no chat hunting)

Subagent events must be structured:
- `progress(percent, message, refs[])`
- `checkpoint(name, status, refs[])`
- `question(question, options?, refs[])`

Manager response is one action:
- `ack/answer` clears `needs_manager` and optionally sets next step.

#### 5.5.1 Required subagent discipline (enforced by the system)

To make this reliable, we enforce:
- Every slice must emit at least one of: progress/checkpoint/question (bounded).
- On `DONE`, output must include proof refs (or it becomes `question` automatically).
- Subagent must not dump logs; only short text + refs.

### 5.6 Proof gate UX (prevent useless CONTINUE loops)

Behavior:
- If job output has no refs but contains proof in text, salvage deterministically.
- If still no proof, convert into a `question` event (explicit `?` in inbox) with a suggested next action.
- Keep override available: manager can accept with `reason+risk`.

---

## 6) Runner plan (24h reliability + multi-runner diagnostics)

### 6.1 Runner state machine (explicit, no heuristics)

Define states:
- `offline`: no heartbeat within TTL
- `idle`: heartbeat alive, not currently holding a slice lease
- `live`: heartbeat alive, actively running a slice

These must be explicit and durable, not guessed from side effects.

#### 6.1.1 Runner parameters (defaults and overrides)

We define explicit knobs:
- `poll_ms`
- `heartbeat_ms`
- `runner_ttl_ms` (derived; e.g., `3 * heartbeat_ms + grace`)
- `slice_s`
- `slice_grace_s`

Defaults must be safe for laptop sleep/resume:
- tolerate short pauses without flipping to offline,
- but detect real dead runners quickly enough for delegation.

### 6.2 Heartbeat

Runner must heartbeat:
- on startup,
- on slice start,
- periodically during slice execution,
- on slice end,
- on graceful shutdown.

### 6.3 Time-slices

Jobs are executed in slices with:
- maximum slice duration (e.g., `slice_s`)
- grace window for shutdown (`slice_grace_s`)

Benefits:
- prevents indefinite monopolization,
- allows reclaim,
- creates manager visibility checkpoints.

### 6.4 Reclaim

If a runner dies:
- lease expires,
- job becomes reclaimable,
- another runner can continue.

Key edge cases:
- avoid reclaim “flapping” under clock drift,
- avoid double-run when runner is slow but alive,
- ensure reclaim is reflected in inbox clearly.

#### 6.4.1 Reclaim policy rules (to avoid false positives)

Reclaim eligibility should depend on:
- lease expiry time (stored),
- last heartbeat time,
- grace window.

We must encode reclaim reason in an event:
- `reclaimed: previous_runner=<id> reason=<ttl_expired|manual|conflict_resolved>`

### 6.5 Multi-runner conflict diagnostics

We need explicit diagnostics for:
- two runners attempting to claim the same job,
- job claimed but no heartbeats (stuck runner),
- runner alive but repeatedly failing slices (bad environment).

Manager should see:
- “who owns lease”
- “last heartbeat”
- “last event ref”
- “suggested action” (requeue / restart runner / accept override).

#### 6.5.1 Multi-runner “always visible who is live and on what”

We add a runners radar view:
- list all runners with state + last heartbeat + current job
- show conflicts (two runners touching same job) as explicit errors, not inferred
- provide one action to “reclaim/requeue”

---

## 7) Tests & evidence plan (how we prove it works)

### 7.1 Contract tests (shape locks)

- `tasks_snapshot` minimal lines stable under truncation.
- `jobs_radar` lines stable and bounded.
- runner status fields present and schema-stable.

### 7.2 Integration tests (behavior)

**Horizon**
- create many tasks, park most, keep 1 active.
- new session snapshot: shows only active + counts.

**Canon**
- publish decision, supersede it, verify smart view shows only latest.

**Delegation**
- fan-out into N jobs, each reports progress and proof, merge report produced.

**Research (long investigation)**
- create an investigation with many hypotheses/tests/evidence (anchor-scoped).
- verify default snapshot stays low-noise and points to the right anchor/ref.
- verify “show drafts” expands only within the selected anchor.

**Runner**
- simulate runner death, reclaim job, ensure no silent stall and no double-done.

### 7.3 DX tests (human “at a glance”)

- “multi-terminal” scenario: two runners + manager terminal.
- verify manager can operate without opening more than 1 ref per job.

### 7.4 Soak tests (24h)

- long-running job that heartbeats correctly,
- forced restarts,
- verify manager inbox always reflects truth (no false live).

#### 7.4.1 Reliability drills (edge cases)

We explicitly simulate:
- runner process kill mid-slice,
- laptop sleep/resume (heartbeat gaps),
- two runners started accidentally with the same workspace,
- job that repeatedly fails to produce proof,
- job that produces proof only in text (not refs),
- budget truncation that would otherwise hide the `last.ref` line.

---

## 8) Rollback / kill-switch strategy

Each layer must be disable-able without data loss:

- Horizon layer: default view filter can be turned off (show all as before).
- Canon layer: superseded filtering can be turned off (show everything).
- Delegation layer: fan-out/fan-in tools can be hidden (manual jobs still work).
- Runner layer: reclaim can be disabled (manual requeue only).

No stored data should become unreadable; only behavior changes.

---

## 9) Implementation sequencing (recommended order)

**Phase 0 — Second Brain Core (make it a “brain”, not a log)**
1) Anchor UX baseline: stable ids + minimal taxonomy + “where=…” in the compass
2) Pack discipline: bounded mindpack/cockpit concept + update cadence rules (focus/decision/proof events)
3) Canon lifecycle rules: publish vs supersede vs hide superseded (discoverable by ref)
4) Minimal plan hygiene lints: active requires next action + anchors required for active (warn, not block)

**Phase 1 — Focus first (highest ROI)**
1) Contracts for horizons + snapshot outputs
2) Storage fields/indexes for horizons
3) Snapshot UX: active/backlog counts + one-command expansion
4) Tests: truncation + horizon behavior

**Phase 2 — Research + canon clarity (second ROI)**
5) Research loop semantics + stop criteria (anchor-scoped)
6) Superseded + pinned cockpit semantics
7) Smart view filters + explicit “show drafts” toggles
8) Tests: no self-contradiction + deep skill slice (research alias)

**Phase 3 — Planning quality**
9) Plan lint rules (DoD/tests/bounded steps) + recovery hints
10) Decomposition playbooks (default templates for research/engineering)
11) Tests: bad plans flagged; good plans stay low-noise

**Phase 4 — Delegation at scale**
12) Feedback protocol + manager ack
13) Fan-out/fan-in + merge report
14) Inbox UX: markers + last.ref + one action
15) Tests: manager flow without chat hunting

**Phase 5 — Runner reliability**
16) Explicit runner liveness
17) Heartbeat + slices + reclaim
18) Multi-runner conflict diagnostics
19) Soak tests / reliability drills

**Phase 6 — Skills (behavior packs)**
20) `skill` tool contracts + versioning
21) Runner injection rules + defaults per profile
22) Tests: skill is bounded + improves adherence (feedback/proof)

### 9.1 Work breakdown structure (WBS) per phase

This is the concrete checklist for implementation work.

#### Phase 0 (Second Brain Core)
- Spec: artifacts + refs-first invariant (what must exist for resume-by-meaning)
- Spec: anchors minimal taxonomy + anchor coverage expectations
- Spec: pack/mindpack/cockpit semantics (bounded, versioned, event-driven cadence)
- Spec: canon lifecycle immutability (supersede, no rewrite) + discoverability rules
- Lints: active requires next; active requires anchor (warn + one-command fix)
- Tests: “resume by meaning” stays navigable under tight budgets

#### Phase 1 (Horizons)
- Spec: horizon states + defaults + truncation invariants
- Storage: schema + migrations + indexes
- Tools: set/list/snooze/promote + snapshot additions
- Views: smart default + explicit backlog expansion
- Lints: active>3, stale noise, “active but no next”
- Tests: contract + integration + tiny-budget snapshot

#### Phase 2 (Research + Canon)
- Spec: research loop + stop criteria + anchor-scoped retrieval
- Spec: supersede rules + selection order + discoverability by ref
- Storage/graph: superseded relation + bounded research indexing
- Views: smart shows only canon + current frontier; drafts opt-in by anchor
- “Cockpit”: pinned canon note pattern (per task/anchor)
- Tests: long investigation stays navigable; no self-contradiction

#### Phase 3 (Planning quality)
- Spec: plan lint rules (DoD/tests/boundedness) + typed recovery hints
- Playbooks: decomposition templates (research/engineering/teamlead)
- Views: snapshot suggests “best next decomposition move” when plan is weak
- Tests: bad plans flagged; good plans pass without extra bureaucracy

#### Phase 4 (Delegation)
- Spec: job schema, feedback protocol, manager ack/answer (one action clears `needs_manager`)
- Fan-out macro: split by anchors (3–10 jobs) + budgets per job
- Inbox UX: `? ! ~ ✓` markers + `runner_id` + explicit `live|idle|offline` + `last.ref` + one action hint
- Proof gate UX: salvage proof-from-text → refs; else convert to explicit `question` (no silent CONTINUE loops)
- Fan-in macro: merge report schema (changes, proofs, risks, next+backup, supersedes/merges)
- Tests: fanout+fanin flow; missing proof ⇒ question; manager ack clears; “at-a-glance” inbox readability

#### Phase 5 (Runner reliability)
- Spec: runner registration + durable heartbeat + explicit `live|idle|offline` state machine (no heuristics)
- Heartbeats: TTL computation + laptop sleep/resume tolerance (grace) without false offline spam
- Time-slices: slice lease model + checkpoints + bounded progress events
- Reclaim: expiry rules + reclaim reasons + anti-flap safeguards (no double-run)
- Multi-runner diagnostics: conflict detection + “always visible who is live and on what”
- Soak drills: 24h job, kill mid-slice, restart, sleep/resume, double-runner start
- Tests: reclaim correctness + false-offline near zero + deterministic radar output under truncation

#### Phase 6 (Skills)
- Spec: `skill` tool schema + profiles + version pinning
- Runner: inject selected skill pack into subagent prompts (teamlead/deep/strict; research alias accepted)
- DX: packs are short, composable, truncation-safe; include “golden loop” guidance
- Tests: adherence improves (progress/question + DONE⇒proof); skills remain useful under low budgets

---

## 10) “Pinocchio dogfood” checklist (real-world validation)

We validate in a real complex repo to surface DX issues:
- ensure anchors bootstrap quickly (map is not empty),
- ensure focus task has criteria/tests,
- ensure delegation produces merge report with proof,
- ensure runner liveness is unambiguous across terminals.

Pass criteria:
- manager can understand state “on sight” in <15 seconds,
- agent resumes in <30 seconds,
- no repeated reinvention due to hidden canon.

---

## 11) Open questions (explicit frontier)

These are the high-risk decisions to validate early:

1) Horizon storage: stored vs computed (risk: performance vs drift).
2) Stale detection: computed from time vs explicit state (risk: determinism vs usefulness).
3) Canon selection rules: how to break ties deterministically (risk: ambiguity).
4) Runner TTL defaults: avoiding false offline (risk: noisy alerts).
5) Merge report schema: how strict vs flexible (risk: friction vs quality).

For each, we should run a cheap falsifier test before deep implementation.

---

## 12) Backwards-compatibility and migration policy

We do not require backwards compatibility by default, but we do require:
- migrations are forward-safe,
- old data remains readable,
- new fields have safe defaults.

If we change an output shape, we must:
- update contract docs,
- update contract tests,
- ensure low-noise BM‑L1 still works under truncation.
