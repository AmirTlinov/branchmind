#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

cd "${APP_DIR}"

if [[ ! -d node_modules ]]; then
  echo "ERROR: viewer deps missing. Run: make viewer-install" >&2
  exit 1
fi

WEBKIT_DISABLE_DMABUF_RENDERER="${WEBKIT_DISABLE_DMABUF_RENDERER:-1}"
export WEBKIT_DISABLE_DMABUF_RENDERER

run_tauri_dev() {
  npm run tauri:dev
}

run_tauri_dev_x11() {
  GDK_BACKEND=x11 WINIT_UNIX_BACKEND=x11 npm run tauri:dev
}

if [[ "${BM_VIEWER_FORCE_X11:-0}" == "1" ]]; then
  run_tauri_dev_x11
  exit $?
fi

is_wayland=0
if [[ "${XDG_SESSION_TYPE:-}" == "wayland" || -n "${WAYLAND_DISPLAY:-}" ]]; then
  is_wayland=1
fi

if [[ "${is_wayland}" == "0" ]]; then
  run_tauri_dev
  exit $?
fi

tmp_log="$(mktemp -t bm-viewer-tauri-dev.XXXXXX.log)"
cleanup() {
  rm -f "${tmp_log}" 2>/dev/null || true
}
trap cleanup EXIT

set +e
run_tauri_dev 2>&1 | tee "${tmp_log}"
code=${PIPESTATUS[0]}
set -e

if [[ "${code}" == "0" ]]; then
  exit 0
fi

if grep -q "Lost connection to Wayland compositor" "${tmp_log}"; then
  echo
  echo "[viewer] Wayland compositor disconnected. Retrying with X11 fallback..." >&2
  echo "[viewer] Tip: to force X11 directly use: make run-viewer-x11" >&2
  run_tauri_dev_x11
  exit $?
fi

exit "${code}"
