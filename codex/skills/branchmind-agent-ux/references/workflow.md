# Workflow (daily driver)

BranchMind is at its best when you treat it as a **state machine** and let it choose the next action.

## Contents

- Golden loop (SSOT)
- Budget ladder + refs-first navigation
- Starting work (create a focused task)
- Strict planning (before coding)
- Deep branching (risky/architectural)
- Proof-first close
- Delegation (fan-out / fan-in)
- Multi-agent safety (leases)
- Maintenance cadence (daily/weekly/after upgrades)
- Code anchors (repo line refs)

## Golden loop (SSOT)

**Observe → Plan → Branch → Decide → Execute → Prove → Close → Remember**

Minimal daily loop:

1) `branchmind.status`
2) Follow the first suggested `actions[]` (or the first BM‑L1 command line).

## Budget ladder + refs-first navigation (anti-noise)

Default posture:

- `budget_profile=portal`
- `view=compact`
- open things by **ref** instead of asking for “a huge dump”

Escalate only when necessary:

- `portal/compact` → `default/smart` → `audit/audit`

If you see `BUDGET_TRUNCATED`:

1) Copy the returned `ref=<...>` / ids
2) `open` that ref with a larger budget (or rerun the same tool with `budget_profile=default`)

## Starting work (create a focused task)

Use the macro. Keep steps small; 1 step = 1 deliverable.

Tool: `mcp__branchmind__tasks`

### Option A — Explicit steps (fully custom)

```json
{
  "workspace": "my-workspace",
  "op": "call",
  "cmd": "tasks.macro.start",
  "args": {
    "task_title": "Implement <feature>",
    "steps": [
      {
        "title": "Slice A — Plan + contracts",
        "success_criteria": ["Plan is decision-complete", "Edge cases enumerated"],
        "tests": ["make check"]
      },
      {
        "title": "Slice B — Implement + proof",
        "success_criteria": ["Behavior correct", "No regressions"],
        "tests": ["make check"]
      }
    ]
  },
  "budget_profile": "portal",
  "view": "compact"
}
```

Then:
- `tasks.snapshot` (or just call `branchmind.status` again and follow the next action).

### Option B — Built-in templates (fast, disciplined defaults)

For “flagship” work (architectural / risky), prefer:

- `template="flagship-task"` → defaults to `reasoning_mode="deep"` (branching + resolved synthesis required).

Example:

```json
{
  "workspace": "my-workspace",
  "op": "call",
  "cmd": "tasks.macro.start",
  "args": {
    "plan_template": "principal-plan",
    "template": "flagship-task",
    "task_title": "Implement <feature>"
  },
  "budget_profile": "portal",
  "view": "compact"
}
```

Tip (10-year habit): treat templates as your “discipline autopilot”. If a task is risky, choose a stricter
template instead of relying on personal willpower.

## Strict planning (before coding)

Goal: produce a short but complete plan artifact, prove you can verify it, then execute.

Recommended sequence:

0) **Recall-first** (before you touch a component):
   - `think.knowledge.recall(anchor="a:<component>", limit=12)`
   - if you don’t have anchors yet: bootstrap them (see `architecture-map.md`), then recall by anchor.

1) `think.reasoning.seed` (get the deterministic template)
2) Fill the template using `think.reasoning.pipeline` (hypothesis → evidence → decision)
3) Only then proceed to implementation actions

If you are unsure about schemas: `system.schema.get(cmd="think.reasoning.seed")`.

## Deep branching (when the task is risky or architectural)

1) Create **2 alternative branches**:
   - `think.idea.branch.create` (twice)
2) Each branch must include:
   - why it might fail
   - a falsifier test
3) Merge back with a decision:
   - `think.idea.branch.merge`

Templates: `templates.md`.

## Proof-first close (fast path)

Preferred: do it in one call:

- `tasks.macro.close.step` with `proof_input` containing receipts:
  - `CMD: ...`
  - `LINK: ...` (preferred; `file:///...` is ok)

If you need 2-phase:
1) `tasks.evidence.capture`
2) `tasks.step.close`

## Delegation (fan-out / fan-in)

For big tasks, don’t “one-agent” it:

- fan-out: delegate 3–10 slices by anchor / subsystem
- fan-in: merge results into one canonical report that is cheap to resume

Recommended:

- `tasks.macro.delegate` (single slice + job)
- (advanced) `tasks.macro.fanout.jobs` / `tasks.macro.merge.report` (see `delegation.md`)

## Code anchors (repo line refs)

When referencing code in decisions/evidence (or anchor `refs[]`), prefer a **stable, openable**
`code:` ref instead of a raw path:

```text
code:<repo-relative-path>#L<start>-L<end>@sha256:<64-hex>
```

How to use:

1) Open it (gets a bounded snippet + normalized ref):

Tool: `mcp__branchmind__open`

```json
{ "workspace": "my-workspace", "id": "code:crates/mcp/src/main.rs#L10-L42" }
```

2) Copy the returned `ref` (now includes `@sha256:`) into:
   - a decision/evidence card (`Proof:` line),
   - an anchor (`refs[]`),
   - a handoff note.

If the file changes later, reopening the old ref will surface `CODE_REF_STALE` (drift detector).

## Finish

When all steps are closed:
- `tasks.macro.finish` (status DONE + final note + handoff capsule)

## Multi-agent safety (leases)

Rule of thumb: **one writer per step**. Everyone else is reviewer/researcher.

If a mutation fails with `STEP_LEASE_HELD`:

1) Don’t retry blindly.
2) Follow the returned recovery `actions[]` (release / wait / takeover).
3) If you need to claim explicitly, discover schema (don’t guess):
   - `system.schema.get(cmd="tasks.step.lease.claim")`
   - `system.schema.get(cmd="tasks.step.lease.release")`

## Maintenance cadence (keeps UX sharp for years)

- **Daily**: `status` → do next action → attach proof → close step → upsert knowledge when you learn.
- **Weekly**: `think.knowledge.lint` (follow actions to consolidate keys).
- **After upgrades / weird behavior**:
  - check `build=<fingerprint>` in `status`
  - if daemon/proxy feels stale: `system.daemon.restart`
  - sanity check surface: `system.ops.summary`

## Schema-on-demand (escape hatch)

If you see `INVALID_INPUT`:

Tool: `mcp__branchmind__system`

```json
{ "workspace": "my-workspace", "op": "schema.get", "args": { "cmd": "tasks.macro.start" } }
```

Use the returned `example_valid_call` / `example_minimal_args`.
