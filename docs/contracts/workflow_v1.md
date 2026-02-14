[LEGEND]
WORKFLOW = The contract for recurring engineering workflows and gates.

[CONTENT]
Contract: Workflow v1

## Purpose
Define [WORKFLOW]: recurring engineering workflows and what “done” means for each.

## Scope
- In scope: workflow states, gates, definition-of-done, rollback hooks.
- Out of scope: tool choice (CI vendor, tracker).

## Interface
TODO: define workflow state machine and gate hooks.

## Errors
TODO: define invalid state transitions and missing gate evidence.

## Examples
```text
workflow: "change"
states: [draft, review, merged]
gate: ["doctor", "gate"]
```
