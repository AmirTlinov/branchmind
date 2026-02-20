# Contracts — Reasoning Memory Model (v3)

In v3, durable reasoning state is managed only through `branch`, `think`, and `merge`.

## Entities

- **Branch** — named reasoning lane with head pointer.
- **Commit** — immutable thought entry (`commit_id`, optional `parent_commit_id`, `message`, `body`).
- **Merge record** — deterministic synthesis from source branch into target branch.

## Ownership and scope

- Every entity is scoped by `workspace`.
- IDs are stable and caller-controlled where applicable (`branch_id`, `commit_id`).

## Tool responsibilities

- `branch` manages branch lifecycle and listing.
- `think` appends and reads commit history.
- `merge` records synthesis merge operations and emits diagnostics for partial failures.

## History behavior

- `think.log` walks parent links from a cursor (`from`) and returns a bounded page.
- `next_commit_id` points to the first omitted commit (safe pagination continuation).
- `think.delete` is soft delete (tombstone commit), preserving auditability.

## Determinism

- Store-backed only, no remote dependencies.
- Same inputs + same store state produce same logical results.
