[LEGEND]
ORCHESTRATION = The contract for multi-step workflows and execution semantics.

[CONTENT]
Contract: Orchestration v1

## Purpose
Define [ORCHESTRATION]: multi-step workflow execution semantics (including rollback and retries).

## Scope
- In scope: step model, state transitions, retries/timeouts, rollback semantics.
- Out of scope: concrete worker/runtime implementation.

## Interface
TODO: define workflow and step schemas.

## Errors
TODO: define orchestration failures (timeout, non-retryable, rollback-failed).

## Examples
```json
{"workflow":"publish_release","steps":[{"id":"build","retry":2},{"id":"deploy","timeout_s":600}]}
```
