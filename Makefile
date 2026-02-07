.PHONY: help fmt fmt-check clippy test check run-mcp run-viewer-tauri

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
		"  make run-viewer-tauri  Run viewer desktop app (Tauri+Vite+React, optional)"

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

run-viewer-tauri:
	cd viewer-tauri && npm run tauri:dev
