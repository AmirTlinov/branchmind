 [LEGEND]
VERSION = A stable identifier (v1, v2, …) for a contract.
REQUIRED_SECTIONS = The required headings every contract must include.

[CONTENT]
Contract standard:
- Every contract is versioned (e.g., `*_v1.md`).
- The first non-empty line of the contract body starts with `Contract:` and includes the [VERSION].
- Every contract includes the [REQUIRED_SECTIONS] below (exact heading names):
  - `## Purpose`
  - `## Scope`
  - `## Interface`
  - `## Errors`
  - `## Examples` (must include at least one fenced code block)
  - Optional: `## Compatibility`
