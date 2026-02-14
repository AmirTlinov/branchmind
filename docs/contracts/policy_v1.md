[LEGEND]
POLICY = A rule set that constrains agent behavior.

[CONTENT]
Contract: Policy v1

## Purpose
Define [POLICY]: the rule set that constrains agent behavior and defines stop conditions.

## Scope
- In scope: budgets, stop conditions, change protocol invariants, approval rules.
- Out of scope: project-specific technical architecture decisions.

## Interface
TODO: define policy surface (what can be configured; what is enforced).

## Errors
TODO: define policy violation handling (fail closed vs warn; escalation semantics).

## Examples
```text
policy:
  change_protocol: contracts-first
  stop_conditions:
    - secrets_required
    - irreversible_migration
```
