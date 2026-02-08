import { invoke } from "@tauri-apps/api/core";

export async function invokeTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    // The viewer is designed to run inside the Tauri webview. When opened in a regular
    // browser (Vite dev server, `vite preview`, `dist/`), the backend bridge is absent.
    //
    // We deliberately rethrow a clear, copy/paste-ready instruction instead of leaking
    // internal JS errors.
    const hint =
      "Tauri backend is not available. Run `make run-viewer` (repo root) or `cd apps/viewer-tauri && npm run tauri:dev`.";
    const msg = err instanceof Error ? err.message : String(err);
    if (
      msg.includes("TAURI") ||
      msg.includes("tauri") ||
      msg.includes("__TAURI") ||
      msg.includes("not supported")
    ) {
      throw new Error(hint);
    }
    throw new Error(`${hint}\n\nUnderlying error: ${msg}`);
  }
}
