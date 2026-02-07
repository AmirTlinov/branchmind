/* ── SSE Event Stream Hook ── */

import { useEffect, useRef } from "react";
import { useSnapshotStore } from "../store/snapshot-store";
import { useEventsStore } from "../store/events-store";
import { qs } from "../api/client";
import type { SSEPayload } from "../api/types";

const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 30000;

export function useSSE() {
  const project = useSnapshotStore((s) => s.project);
  const workspace = useSnapshotStore((s) => s.workspace);
  const lens = useSnapshotStore((s) => s.lens);
  const refresh = useSnapshotStore((s) => s.refresh);

  const setStatus = useEventsStore((s) => s.setStatus);
  const recordEvent = useEventsStore((s) => s.recordEvent);
  const setLastEventId = useEventsStore((s) => s.setLastEventId);
  const incrementFailures = useEventsStore((s) => s.incrementFailures);
  const resetFailures = useEventsStore((s) => s.resetFailures);

  const failuresRef = useRef(0);

  useEffect(() => {
    const url = `/api/events${qs({ project, workspace, lens })}`;
    let es: EventSource | null = null;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let alive = true;

    function connect() {
      if (!alive) return;
      setStatus("connecting");
      es = new EventSource(url);

      es.onopen = () => {
        if (!alive) return;
        setStatus("connected");
        resetFailures();
        failuresRef.current = 0;
      };

      es.onmessage = (ev) => {
        if (!alive) return;
        setLastEventId(ev.lastEventId || null);
        try {
          const payload = JSON.parse(ev.data) as SSEPayload;
          recordEvent(payload.type, payload);
          // Refresh snapshot on meaningful events
          if (payload.type === "snapshot" || payload.type === "plan" || payload.type === "task") {
            refresh();
          }
        } catch {
          // ignore parse errors
        }
      };

      es.onerror = () => {
        if (!alive) return;
        es?.close();
        setStatus("error");
        incrementFailures();
        failuresRef.current++;
        const delay = Math.min(
          RECONNECT_BASE_MS * Math.pow(2, failuresRef.current - 1),
          RECONNECT_MAX_MS,
        );
        reconnectTimer = setTimeout(connect, delay);
      };
    }

    connect();

    return () => {
      alive = false;
      es?.close();
      if (reconnectTimer) clearTimeout(reconnectTimer);
      setStatus("disconnected");
    };
  }, [project, workspace, lens, refresh, setStatus, recordEvent, setLastEventId, incrementFailures, resetFailures]);
}
