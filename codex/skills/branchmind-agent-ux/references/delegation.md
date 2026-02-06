# Delegation (multi-agent teamwork)

Goal: parallelize work **without** losing context or breaking determinism.

## Contents

- Roles
- Canonical flow (delegate → monitor → merge)
- Manager inbox (`jobs.radar`) + attention markers
- Leases + reclaim (stale runners/jobs)
- Proof gate (DONE means done)

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
4) Attach proof back to the parent task step (`jobs.proof.attach` or `tasks.evidence.capture`)

For big tasks:

- fan-out: `tasks.macro.fanout.jobs` (3–10 jobs is the sweet spot)
- fan-in: `tasks.macro.merge.report` (one canonical resume artifact)

## Communication rules (anti-noise)

- Always send **refs-first** (TASK/STEP/CARD ids) instead of pasting large text.
- Ask for **proof receipts**:
  - `CMD:` what was run
  - `LINK:` to logs/artifacts (file links ok)
- If the delegate discovers reusable invariants → they must `think.knowledge.upsert` with `(anchor,key)`.

## Manager inbox: jobs.radar (glanceable supervision)

Use `jobs.radar` as the “teamlead HUD”. It uses deterministic markers:

- `!` — attention (error / proof gate / blocking)
- `?` — needs manager decision (agent asked)
- `~` — stale (RUNNING but claim lease expired; reclaimable)

Golden move: if you don’t know what to do, open the newest event ref from the radar row.

## Leases + reclaim (no heuristics)

Over years, runners die, terminals close, and jobs get stranded. BranchMind handles this explicitly:

- runner liveness is an explicit lease (heartbeat)
- RUNNING jobs have an explicit **claim lease** (time-slice)

When you see `~` (stale job):

1) `jobs.open` to confirm it’s truly stale and see last events
2) reclaim via `jobs.claim allow_stale=true` (follow recovery actions; don’t guess args)
3) continue with `jobs.report` / `jobs.complete`

## Proof gate (DONE means done)

Delegated work is only “done” when it is verifiable:

- Must return stable refs (`CARD-*`, `notes_doc@seq`, `TASK-*`, `JOB-*`) and
- Must provide at least one proof receipt (`CMD:` and/or `LINK:`) when applicable.

Best practice:

- store findings as cards/notes
- then use `jobs.proof.attach` to attach those receipts to the parent step

## Monitoring loop

- `branchmind.status` (next actions)
- `branchmind.jobs op="call" cmd="jobs.radar"` (glanceable job state)
- `branchmind.jobs op="call" cmd="jobs.tail"` (when you need progress logs)
  - use `jobs.wait` when you need bounded polling (timeout returns success + done=false)

## See also

- Teamlead protocol: `teamlead.md`
- Portable anchors + keys taxonomy: `taxonomy.md`
