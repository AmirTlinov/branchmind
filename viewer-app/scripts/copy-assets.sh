#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
VIEWER_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ASSETS_DIR="$(cd "$VIEWER_DIR/../crates/mcp/src/viewer/assets" && pwd)"

echo "==> Building viewer..."
cd "$VIEWER_DIR"

echo "==> Copying assets to $ASSETS_DIR"
cp "$VIEWER_DIR/dist/index.html" "$ASSETS_DIR/index.html"

echo "==> Done. Assets:"
ls -lh "$ASSETS_DIR/"
