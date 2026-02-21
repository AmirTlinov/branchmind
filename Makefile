.PHONY: help fmt fmt-check clippy test check run-mcp shared-reset

CARGO ?= cargo

help:
	@printf "%s\n" \
		"Targets:" \
		"  make check      Run fmt-check + clippy + tests" \
		"  make fmt        Apply rustfmt" \
		"  make fmt-check  Verify formatting" \
		"  make clippy     Run clippy (deny warnings)" \
		"  make test       Run workspace tests" \
		"  make run-mcp    Run MCP server (DX defaults)" \
		"  make shared-reset  Reset current shared socket (best-effort daemon shutdown + stale socket cleanup)" \
		""

fmt:
	$(CARGO) fmt

fmt-check:
	$(CARGO) fmt --check

clippy:
	$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings

test:
	$(CARGO) test --workspace

check: fmt-check clippy test

# Golden path: zero-arg run enables DX defaults.
run-mcp:
	$(CARGO) run -p bm_mcp

shared-reset:
	$(CARGO) run -p bm_mcp -- --shared-reset
