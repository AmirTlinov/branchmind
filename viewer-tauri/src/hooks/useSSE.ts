/* ── Live updates hook (polling; desktop viewer routes HTTP via Tauri backend) ── */

import { useEffect } from "react";
import { useSnapshotStore } from "../store/snapshot-store";
import { useEventsStore } from "../store/events-store";

const POLL_INTERVAL_MS = 2000;

export function useSSE() {
  const project = useSnapshotStore((s) => s.project);
  const workspace = useSnapshotStore((s) => s.workspace);
  const lens = useSnapshotStore((s) => s.lens);
  const refresh = useSnapshotStore((s) => s.refresh);

  const setStatus = useEventsStore((s) => s.setStatus);
  const incrementFailures = useEventsStore((s) => s.incrementFailures);
  const resetFailures = useEventsStore((s) => s.resetFailures);

  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setInterval> | null = null;

    async function tick() {
      if (!alive) return;
      try {
        await refresh();
        if (!alive) return;
        setStatus("connected");
        resetFailures();
      } catch {
        if (!alive) return;
        setStatus("error");
        incrementFailures();
      }
    }

    setStatus("connecting");
    void tick();
    timer = setInterval(() => void tick(), POLL_INTERVAL_MS);

    return () => {
      alive = false;
      if (timer) clearInterval(timer);
      setStatus("disconnected");
    };
  }, [
    project,
    workspace,
    lens,
    refresh,
    setStatus,
    incrementFailures,
    resetFailures,
  ]);
}
