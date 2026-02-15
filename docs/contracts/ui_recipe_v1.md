[LEGEND]
UI_RECIPE = The contract for UI flows and acceptance checks.

[CONTENT]
Contract: UI recipe v1

## Purpose
Define [UI_RECIPE]: a minimal format for describing UI flows and acceptance checks.

## Scope
- In scope: flow steps, acceptance checks, regression checklist.
- Out of scope: implementation details (framework/components).

## Interface
TODO: define a UI recipe schema (steps, asserts, fixtures).

## Errors
TODO: define invalid flow/step errors.

## Examples
```yaml
flow:
  name: "Create item"
  steps:
    - action: "click"
      target: "New"
    - action: "assert"
      text: "Item created"
```
