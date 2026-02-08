import { invoke } from "@tauri-apps/api/core";

export async function invokeTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);

    // When opened in a regular browser (Vite dev server / preview / dist),
    // the IPC bridge is absent. In that case we want a clean instruction.
    const hint =
      "Tauri backend is not available. Run `make run-viewer` (repo root) or `cd apps/viewer-tauri && npm run tauri:dev`.";
    const looks_like_not_in_tauri =
      msg.includes("__TAURI") ||
      msg.includes("not supported") ||
      msg.toLowerCase().includes("tauri") ||
      msg.toLowerCase().includes("ipc");
    if (looks_like_not_in_tauri) {
      throw new Error(hint);
    }

    // Otherwise, surface the real backend error (invalid args, missing schema, etc).
    throw new Error(msg);
  }
}
