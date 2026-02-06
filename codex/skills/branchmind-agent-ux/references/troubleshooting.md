# Troubleshooting (low-noise)

## INVALID_INPUT

Do not guess fields.

- `system.schema.get(cmd=...)` and follow the returned `example_valid_call`.

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

## Legacy tool names

BranchMind v1 surface is portal-first. If you see older names in copied text, treat them as unknown and use:
- `tools/list`
- `system.ops.summary`
- `system.schema.get`

