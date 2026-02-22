# Contracts — v3 Integration Invariants

This document defines how `branch`, `think`, and `merge` compose into one deterministic workflow.

## Core invariants

1. **Single workspace scope**
   - Every write/read is scoped to explicit `workspace`.
2. **Branch-first history**
   - `think.commit` writes to an existing branch and advances branch head.
3. **Deterministic pagination**
   - `think.log.next_commit_id` is the first omitted item, never skipped.
4. **Merge diagnostics are preserved**
   - Partial merge failures return structured warnings.
   - Total merge failure (`MERGE_FAILED`) still returns warnings/failures detail.
5. **Fail-closed surface**
   - Unknown tool/verb/arg shapes are rejected explicitly.

## Minimal end-to-end flow

1. Create main lane:
   - `branch` + ` ```bm\nmain\n``` `
2. Add commits:
   - `think` + ` ```bm\ncommit branch=main commit=c1 message=... body=...\n``` `
3. Inspect history:
   - `think` + ` ```bm\nlog branch=main limit=20\n``` `
4. Merge feature lane:
   - `merge` + ` ```bm\ninto target=main from=feature strategy=squash\n``` `

## Out of scope

- Removed multi-portal surfaces and non-current alias tools.
- Non-markdown tool inputs.
