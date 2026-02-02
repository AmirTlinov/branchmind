# Agent UX Audit (v1 portals/actions-first) — 2026-02-02

Scope: BranchMind v1 “portal” UX loop (10 tools) + actions-first recovery + bounded outputs.

Refs (runtime):
- TASK-012 / JOB-003 (UX audit job thread)
- a:audit (anchor)

Proof receipts:
- CMD: make check

---

## 1) Current surface metrics (from `system.ops.summary`)

- **Portal tools:** 10
  - `docs`, `graph`, `jobs`, `open`, `status`, `system`, `tasks`, `think`, `vcs`, `workspace`
- **Golden ops:** 31 total
  - by tool (golden path): `docs=4`, `graph=3`, `jobs=5`, `system=4`, `tasks=5`, `think=7`, `vcs=1`, `workspace=2`
- **Cmd registry:** 163 total
  - by domain: `tasks=65`, `think=46`, `jobs=12`, `system=9`, `graph=8`, `docs=8`, `vcs=13`, `workspace=2`
- **Unplugged ops:** none detected (`unplugged=[]`)

---

## 2) What works well (low cognitive load)

### Portal-first, copy/paste loop
The “L1 line” outputs (`status`, `tasks.snapshot`, `jobs.radar`) provide an immediate:
- **now** (focus),
- **next** (single runnable action),
- **backup** (secondary action),
without requiring a scroll-heavy dump.

### Typed errors + self-recovery hints
Two key failure modes are handled “cheaply”:
- **Budget exceed** errors provide a concrete recovery hint (e.g., “Use limit <= 50”).
- **INVALID_INPUT** errors can guide to `system.schema.get(cmd)` to discover the exact args schema.

### Deterministic repo gates
Local golden path commands remain stable and bounded:
- `make check` runs fmt-check + clippy(-D warnings) + tests.

---

## 3) UX friction (with evidence)

### 3.1 `tasks.lint`: patch selection under low `patches_limit` was low-ROI

Problem:
- When `patches_limit` is small (common in strict agent profiles), `tasks.lint` could return
  confirmation patches (`confirm_criteria`, `confirm_tests`) while **dropping** the more critical
  “keep the task executable after /compact” patches:
  - `set_next_action`
  - `require_proof_tests`
  - `missing_anchor`

Impact:
- Agents lose the executable “next move” and proof gating guidance right when truncation happens.

Fix status:
- **Implemented** (see §4.1).

### 3.2 `jobs.report` ergonomics: required fields are non-obvious without schema discovery

Observation:
- `jobs.report` requires `job + runner_id + claim_revision + message` (and optionally kind/percent/refs).
- In practice, `runner_id` and `claim_revision` are obtained via `jobs.open` (or by remembering them),
  which makes the minimal loop slightly more “ops-heavy” than it needs to be.

Fix status:
- **Proposed** (see §4.2), contract-stable via additive `actions[]` suggestions.

### 3.3 Paging ergonomics: `system.cmd.list` is correctly budgeted, but could be more “one-hop”

Observation:
- The API correctly rejects large `limit` values, and returns `pagination.next_cursor`.
- However, the UX could be improved by emitting a copy/paste “next page” action automatically.

Fix status:
- **Proposed** (see §4.3), contract-stable via additive `actions[]` suggestions.

---

## 4) High-ROI improvements (contracts stable, bounded outputs)

### 4.1 Implemented: `tasks.lint` patch prioritization when `patches_limit` truncates

Change:
- When `select_patches()` truncates, prioritize:
  1) `set_next_action`
  2) `require_proof_tests`
  3) `missing_anchor`
  …over confirm-only patches (`confirm_criteria`, `confirm_tests`).

Why this is high ROI:
- Under truncation, the agent still gets the **single next executable action** and the **proof gate**
  needed to keep “DONE means DONE”.

Minimal diff:
- `crates/mcp/src/tools/tasks/views/admin/lint.rs` (priority ordering only)
- `crates/mcp/tests/tasks_contract/flows/lint_patch_suggestions.rs` (new regression test)

### 4.2 Proposed: `jobs.open` should emit a prefilled `jobs.report` action skeleton

Change (no contract changes):
- In `jobs.open`, add an `actions[]` item: a copy/paste-ready call to `jobs.report` with:
  - `job` = current job id
  - `runner_id` = current runner id
  - `claim_revision` = current job revision token
  - `message` placeholder

Benefit:
- Enables a one-hop “report progress” loop without schema hunting or argument guessing.

Risk:
- None to contracts (additive suggestion). Slightly increases response bytes; must stay bounded.

### 4.3 Proposed: auto “next page” `actions[]` for list endpoints with pagination

Candidates:
- `system.cmd.list`
- `docs.list`, `graph.query`, `jobs.list`, etc.

Change:
- When `pagination.has_more=true`, emit an `actions[]` item with the same call but `offset=next_cursor`.

Benefit:
- Removes a common “mental bookkeeping” step for agents/humans.

Risk:
- None to contracts (additive suggestion). Must keep `actions[]` small (≤1).

---

## 5) Risks / tradeoffs

- Reprioritizing lint patches means confirm-only patches may be hidden at very low `patches_limit`.
  Mitigation: they remain available with a higher `patches_limit` (or full view).
- Adding more `actions[]` suggestions is powerful but can increase noise if abused; keep them
  bounded and only emit when clearly relevant (pagination or common “next” moves).

