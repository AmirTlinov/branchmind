# Delegation (multi-agent teamwork)

Goal: parallelize work **without** losing context or breaking determinism.

## Roles (recommended)

- `planner` — produce decision-complete plan + risks + verify plan
- `researcher` — gather facts/options + citations/refs
- `implementer` — execute a scoped slice with proofs
- `reviewer` — adversarial review, find risks/regressions
- `devils_advocate` — generate counter-hypotheses and falsifiers

## One command (preferred): tasks.macro.delegate

Use the macro so the server can attach the right context (refs-first) deterministically.

1) Create delegation job
2) Monitor via `jobs.radar`
3) Collect results via `jobs.open` / `open` refs
4) Attach proof back to the parent task step (`tasks.evidence.capture`)

## Communication rules (anti-noise)

- Always send **refs-first** (TASK/STEP/CARD ids) instead of pasting large text.
- Ask for **proof receipts**:
  - `CMD:` what was run
  - `LINK:` to logs/artifacts (file links ok)
- If the delegate discovers reusable invariants → they must `think.knowledge.upsert` with `(anchor,key)`.

## Monitoring loop

- `branchmind.status` (next actions)
- `branchmind.jobs op="call" cmd="jobs.radar"` (glanceable job state)
- `branchmind.jobs op="call" cmd="jobs.tail"` (when you need progress logs)

