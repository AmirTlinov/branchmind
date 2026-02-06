# Workflow (daily driver)

BranchMind is at its best when you treat it as a **state machine** and let it choose the next action.

## Golden loop (SSOT)

**Observe → Plan → Branch → Decide → Execute → Prove → Close → Remember**

Minimal daily loop:

1) `branchmind.status`
2) Follow the first suggested `actions[]` (or the first BM‑L1 command line).

## Starting work (create a focused task)

Use the macro. Keep steps small; 1 step = 1 deliverable.

Tool: `mcp__branchmind__tasks`

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

## Strict planning (before coding)

Goal: produce a short but complete plan artifact and then execute.

Recommended sequence:

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

## Finish

When all steps are closed:
- `tasks.macro.finish` (status DONE + final note + handoff capsule)

## Schema-on-demand (escape hatch)

If you see `INVALID_INPUT`:

Tool: `mcp__branchmind__system`

```json
{ "workspace": "my-workspace", "op": "schema.get", "args": { "cmd": "tasks.macro.start" } }
```

Use the returned `example_valid_call` / `example_minimal_args`.

