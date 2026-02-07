export async function invokeTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const invoke = (window as any).__TAURI__?.core?.invoke as
    | ((command: string, args?: any) => Promise<any>)
    | undefined;

  if (!invoke) {
    throw new Error("Tauri backend is not available. Run `npm run tauri:dev`.");
  }
  return (await invoke(cmd, args)) as T;
}

