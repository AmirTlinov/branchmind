[LEGEND]
COGNITION = The contract for how reasoning artifacts are stored and linked.

[CONTENT]
Contract: Cognition v1

## Purpose
Define [COGNITION]: how reasoning artifacts are recorded, linked, and retrieved.

## Scope
- In scope: artifact types, linking rules, minimal required fields.
- Out of scope: UI/visualization and storage backend.

## Interface
TODO: define artifact envelope and required fields per type.

## Errors
TODO: define invalid artifact/link errors.

## Examples
```json
{"type":"evidence","title":"Gate green","links":["decision:use-content-only"]}
```
