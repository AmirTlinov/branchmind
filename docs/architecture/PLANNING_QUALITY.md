# Planning Quality System (incremental, multi-plan, non-bureaucratic)

This document specifies **how BranchMind improves planning quality** without turning into
bureaucracy, and how agents can **incrementally refine** a plan instead of rewriting it repeatedly.

The design is intentionally **AI-native**:
- the default surface is small (BM‑L1 compass),
- deeper detail is available on demand (progressive disclosure),
- “bad” plans are not rejected — they become **refactorable objects**.

---

## 1) Goals / Non-goals

### Goals

1) **Executable plans by default**
   - Every active plan has a clear *next action*, and a way to know “done”.
2) **Incremental refinement**
   - A plan can be improved via small patches (no re-authoring from scratch).
3) **Multi-plan composition**
   - A “plan” may contain multiple **detailed subplans**, typically split by architecture anchors.
4) **Research-grade workflows**
   - Long investigations remain navigable: hypothesis→test→evidence→decision is first-class.
5) **Low-noise resumption**
   - New sessions show only 1 focus + next + backup, plus counts of hidden backlog.

### Non-goals

- Perfect automation (auto-planning that is always correct).
- Enforcing a single “style” of planning.
- Making planning a hard gate everywhere (soft by default; strict profile can gate).

### How this maps to the flagship guarantees

This document primarily implements:
- **G3 (Principal‑grade planning & decomposition by default)**: plans are refactorable artifacts, and
  quality is improved via lint → one-command patches, not rewrites.

And it supports:
- **G1 (Anti‑kasha)**: only 1–3 items are active by default; everything else is hidden but reachable by `ref`.

---

## 2) Core thesis: plans are refactorable artifacts

BranchMind treats a plan as a **persistent structure** with stable identifiers.

Instead of “write a new plan”, agents should do:

**Plan → Lint → Patch (small) → Repeat**

This is the same idea as code refactoring:
- we don’t rewrite a codebase when it’s imperfect,
- we apply small, verifiable edits with a diff trail.

---

## 3) Plan representation (Plan IR)

We model planning as a tree + metadata.

### 3.1 Entities

**Task**
- top-level unit (initiative). It can act as:
  - a plan container (“epic”),
  - a subplan (“work package”),
  - a research notebook (“investigation”).

**Step**
- atomic unit of execution intent.
- always addressable by a stable `step_id`.

**Plan Bundle**
- a task that owns a high-level outline, and links to multiple child tasks as subplans.

### 3.2 Step kinds (pays rent in reasoning)

Each step declares a `kind` (or is inferred):
- `research` (explore unknowns; produce evidence or a decision)
- `design` (produce a decision + constraints)
- `implement` (produce changes + proof)
- `verify` (produce tests/measurements + proof)
- `ops` (deployment/maintenance)

Why this matters:
- different step kinds have different “minimum acceptable proof”.

### 3.3 Minimum fields per step (v1)

For a step to be eligible for **Active Horizon**, it should meet the **Principal Plan Standard**:

- `title` (short, action-oriented)
- `Goal` (1 line: what changes in the world)
- `DoD / success_criteria[]` (1–3 testable bullets)
- `Next action` (copy/paste-ready; one concrete move)
- `Proof plan` (`tests[]` or `proof_expected[]`: what evidence makes DONE true)
- `Risks[]` (≤3) with a cheap mitigation action each
- `anchors[]` (1–3 anchors; “where in architecture”)

Research steps additionally require:
- `falsifier` (fastest way to be wrong)
- `stop criteria` (time/budget/signal)

Incomplete steps are allowed, but they should be:
- backlog/parked by default, or
- explicitly marked as `research` with stop criteria.

---

## 4) What is a “bad plan” (issue taxonomy)

BranchMind should never say “invalid plan” as a dead end.
Instead it produces **issues** with severity and **patch suggestions**.

### 4.1 Issue classes

**A) Unverifiable**
- missing DoD / success criteria
- criteria are non-testable (“make it better”)

**B) Unproveable**
- no tests/proof expectation
- “DONE” without receipts is likely

**C) Unbounded**
- step too large (multiple concerns, unclear boundaries)
- no stop criteria (especially research)

**D) Unnavigable**
- no anchors (cannot scope context)
- mixes unrelated architecture areas

**E) Self-contradicting**
- multiple canon decisions conflict
- no superseded links

**F) Actionless**
- step has no next action / is “meta-only”

### 4.2 Severity levels

- `hint`: improvement suggestion (no impact on closing)
- `warn`: likely to cause confusion / churn
- `block_strict`: blocks strict-gated close unless override is provided

---

## 5) Lint output must be a patch generator (not a verdict)

### 5.1 Lint output structure (conceptual)

Lint returns:
- `issues[]` (what is wrong, where, severity)
- `patches[]` (small candidate edits)
- `actions[]` (copy/paste macro calls to apply patches)

### 5.2 Patch suggestions (example types)

- `add_success_criteria(step_id, bullets[])`
- `add_tests(step_id, tests[])`
- `split_step(step_id, into=[...])`
- `extract_subplan(step_id, anchors=[...])`
- `attach_anchor(step_id, anchor_id)`
- `set_stop_criteria(step_id, time_budget, signal)`
- `mark_decision_superseded(old, new)`

Crucial: patches must be **small** and **composable**.

---

## 6) Incremental refinement workflow (the golden loop)

### 6.1 Fast start (skeleton first)

When creating a task:
- create 3–7 steps with titles only (outline),
- do not over-fill details upfront.

Then:
- promote only 1–3 steps to Active Horizon,
- refine those steps with lint-driven patches.

### 6.2 Refine-in-place (no rewriting)

Agents improve by:
- adding criteria/tests to an existing step,
- splitting a step,
- extracting subplans,
- superseding earlier decisions.

All changes are captured as durable events, so we keep a “why it changed” trail.

### 6.3 Progressive disclosure (anti-bureaucracy)

The default view shows:
- focus + next + backup,
- the top 1–3 anchors involved,
- only the canon decisions + minimal evidence refs.

Everything else is opt-in:
- drafts,
- full history,
- full backlog.

---

## 7) Multi-plan composition (“plan of plans”)

### 7.1 When to split into subplans

Split when:
- steps touch distinct anchors with weak coupling,
- work can be delegated in parallel,
- investigation requires independent notebooks (different evidence sets).

### 7.2 Plan Bundle pattern

We recommend:
- **Root task** = “outline + steering wheel”
  - contains 5–12 high-level steps (the table of contents),
  - each step links to a child task (subplan) per anchor.

Example:
- Root: “Improve Runner Reliability”
  - Step: “Heartbeat semantics” → Subplan task anchored to `a:runner-heartbeat`
  - Step: “Reclaim” → Subplan task anchored to `a:runner-reclaim`
  - Step: “Radar UX” → Subplan task anchored to `a:runner-radar`

### 7.3 Navigation rule

Managers/agents should navigate by:
- anchor id (meaning),
- stable `ref` IDs,
- not by file paths.

---

## 8) Research-grade investigations (scientific loop)

Research steps must include:
- hypothesis (what we believe)
- falsifier test (fastest way to be wrong)
- evidence (what we observed)
- decision (what we commit)
- stop criteria (time/budget/signal)

This is how BranchMind improves “genius” ideas:
- ideas must become falsifiable,
- proof becomes explicit,
- the system remembers why we chose a path.

---

## 9) Built-in skills & playbooks (behavior shaping)

To make this repeatable across sessions/subagents:

- `skill(profile=daily)` keeps the compass loop tight.
- `skill(profile=research)` enforces falsifiers + stop criteria.
- `skill(profile=teamlead)` enforces feedback protocol and proof-first.

Skills must be:
- versioned (pin-able),
- short by default,
- composable via progressive disclosure.

---

## 10) Proof plan (how we validate planning quality)

We measure product outcomes:
- new session resume time
- number of actions shown in snapshot
- stale/backlog noise rate
- reinvention rate (via duplicates/supersedes)
- delegation autonomy rate

And we validate behavior via tests:
- contract tests: lint output schema + patch schema stable
- integration tests: “bad plan → incremental patches → good plan”
- DX tests: “plan bundle” remains navigable at a glance
