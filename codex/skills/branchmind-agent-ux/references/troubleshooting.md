# Troubleshooting (low-noise)

## Contents

- INVALID_INPUT
- UNKNOWN_TOOL / legacy tool names
- BUDGET_* warnings / truncated output
- PROOF_REQUIRED / checkpoints missing
- STEP_LEASE_* (multi-agent collisions)
- REVISION_MISMATCH / EXPECTED_TARGET_MISMATCH
- Runner offline / stale jobs
- “Feels stale” after rebuild (daemon restart)

## INVALID_INPUT

Do not guess fields.

- `system.schema.get(cmd=...)` and follow the returned `example_valid_call`.

## UNKNOWN_TOOL / legacy tool names

BranchMind v1 surface is portal-first (10 tools). If copied text uses legacy names:

1) run `tools/list` (or `system.ops.summary`) to confirm the surface
2) find the right cmd via `system.cmd.list(q="tasks.")`
3) fetch exact schema via `system.schema.get(cmd="...")`

## BUDGET_* warnings / truncated output

- Re-open the referenced artifact via `open` with a larger budget profile (`default`/`audit`).
- Prefer refs-first navigation over requesting huge views.

## PROOF_REQUIRED / checkpoints missing

- Attach receipts first:
  - `CMD: <command>`
  - `LINK: <log/artifact>` (preferred)
- Use `tasks.evidence.capture`, then retry closing the step.

## STEP_LEASE_HELD

- Ask the holder agent to release,
- wait for expiry,
- or explicitly take over (follow the recovery actions).

Related:

- `STEP_LEASE_NOT_HELD` — you attempted a release/renew without owning the lease; follow recovery actions.

## REVISION_MISMATCH / EXPECTED_TARGET_MISMATCH

These are normal in concurrent workflows.

- Don’t retry blindly.
- Refresh state (`status` / `tasks.snapshot` / `open`), then re-apply with the updated revision/target.
- Prefer executing recovery `actions[]` over inventing retries.

## Runner offline / stale jobs (~)

If delegation looks stuck:

- Use `jobs.radar` first (it encodes attention markers deterministically: `! ? ~`).
- If runner is offline, try `jobs.runner.start` (or follow the suggested bootstrap command).
- If a job is `~` stale (claim lease expired), reclaim via `jobs.claim allow_stale=true` (follow actions).

## “Feels stale” after rebuild (daemon restart)

If you rebuilt the binary but behavior looks old:

1) check `build=<fingerprint>` in `branchmind.status`
2) run `system.daemon.restart` (shared mode escape hatch)
3) retry `status` and confirm the fingerprint changed
