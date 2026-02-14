[LEGEND]
EVENT_LOG = The contract for durable events and their schema/versioning.

[CONTENT]
Contract: Event log v1

## Purpose
Define [EVENT_LOG]: the durable event envelope and rules for versioning, retention, and replay.

## Scope
- In scope: event envelope, versioning rules, retention, idempotency.
- Out of scope: concrete storage implementation.

## Interface
TODO: define event envelope fields and schema evolution rules.

## Errors
TODO: define error modes (invalid event, version mismatch, retention violations).

## Examples
```json
{"type":"example.event.v1","ts":"2026-01-07T00:00:00Z","data":{"id":"123"}}
```
