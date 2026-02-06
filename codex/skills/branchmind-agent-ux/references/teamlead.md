# Teamlead protocol (multi-agent, 3–10 agents)

Goal: scale throughput **without** losing narrative, proof, or determinism.

BranchMind makes this possible by turning work into a *ref-first state machine*:
you don’t “remember”; you `open` and you follow `actions[]`.

## The manager HUD (always start here)

1) `branchmind.status` — global orientation + best next action
2) `jobs.radar` — delegation inbox with deterministic attention markers (`! ? ~`)
3) `open <ref>` — inspect only the one thing that needs attention

Rule: if you don’t know what to do, open the newest ref from the radar row.

## Triage semantics (jobs.radar markers)

- `!` attention: error / proof gate / blocking
- `?` agent asked for a decision (needs manager input)
- `~` stale: RUNNING but claim lease expired (reclaimable)

Golden action order:

1) `?` (decisions unblock work fastest)
2) `!` (errors/proof gates prevent completion)
3) `~` (stale jobs silently burn time)

## Canonical manager loop (15 minutes / day)

1) `branchmind.status` → do the first `actions[]` if it’s high-priority/cheap.
2) `jobs.radar` → for each `?` job:
   - open its latest ref
   - reply with a *decision packet* (template below)
3) For each `!` job:
   - open latest ref
   - ask for proof receipts or fix the contract mismatch
4) For `~` jobs:
   - reclaim via `jobs.claim allow_stale=true` (follow recovery actions; don’t guess schema)

## Decision packets (manager → agent)

Use this shape. It prevents 10-message back-and-forth loops.

```text
Context (refs): JOB-... TASK-... STEP-... CARD-... code:...
Decision needed:
Options (2-4):
Recommended option:
Constraints:
Stop criteria:
Proof required (CMD/LINK/ref):
```

## Fan-out / fan-in (the scalable pattern)

### Fan-out: split by meaning (anchors), not by files

Split a large initiative into 3–10 parallel slices by anchors:

- `a:core` (domain logic)
- `a:storage` (persistence)
- `a:mcp` (adapter / schema / budgets)
- `a:runner` (delegation protocol)
- `a:docs` (contracts + UX doctrine)

Preferred tools:

- `tasks.macro.delegate` for a single slice + job
- `tasks.macro.fanout.jobs` for batch fan-out (when available in your workflow)

### Fan-in: one canonical artifact to resume from

Delegation is logistics, not memory. The end state must be a *single* canonical thing:

- one step (or report) that summarizes:
  - what’s done
  - what remains
  - where the proofs are (refs)
  - the next action

Preferred tools:

- `tasks.macro.merge.report` (canonical merge)
- `jobs.proof.attach` to attach receipts back to the parent step

## Quality gates (how to keep “DONE means done”)

Non-negotiable:

- every closed step has proof receipts (`CMD:` and/or `LINK:`)
- every risky/architectural step has a resolved decision (use `reasoning_mode=deep`)
- “DONE without refs” is not DONE in delegation (require at least one `CARD-*` or `notes_doc@seq`)

Manager tactic:

- treat “missing proof” as a normal defect
- reply with *exact* missing items (not “add more details”)

## Multi-agent safety (leases + single writer)

Rule of thumb: **one writer per step**.

If you hit `STEP_LEASE_HELD`:

- don’t retry blindly
- follow recovery actions
- as a manager: decide who owns the step; everyone else becomes reviewer/researcher

## Knowledge governance (prevents long-term rot)

Weekly:

- run `think.knowledge.lint` and follow its actions
- consolidate duplicate keys, split overloaded keys
- enforce `Expiry:` on durable cards (stale truth is worse than no truth)

## “After upgrade / feels stale” playbook

If behavior looks old after rebuild:

1) check `build=<fingerprint>` in `branchmind.status`
2) run `system.daemon.restart`
3) re-check `status` fingerprint

## Metrics (simple, high-signal)

- **Resume time**: `status` → useful action ≤ 90s
- **Proof completeness**: closed steps have CMD/LINK
- **Back-and-forth rate**: a job reaches DONE with ≤ 2 manager messages
- **Lint noise**: weekly lint issues should trend down, not up

