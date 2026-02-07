/* ── Events Store (SSE connection state) ── */

import { create } from "zustand";

export type SSEStatus = "disconnected" | "connecting" | "connected" | "error";

export interface EventsState {
  status: SSEStatus;
  lastEventId: string | null;
  lastEventAt: number;
  failures: number;
  eventLog: Array<{ type: string; data: unknown; ts: number }>;

  setStatus: (status: SSEStatus) => void;
  recordEvent: (type: string, data: unknown) => void;
  setLastEventId: (lastEventId: string | null) => void;
  incrementFailures: () => void;
  resetFailures: () => void;
}

export const useEventsStore = create<EventsState>((set) => ({
  status: "disconnected",
  lastEventId: null,
  lastEventAt: 0,
  failures: 0,
  eventLog: [],

  setStatus: (status) => set({ status }),
  recordEvent: (type, data) =>
    set((s) => ({
      eventLog: [...s.eventLog.slice(-99), { type, data, ts: Date.now() }],
      lastEventAt: Date.now(),
    })),
  setLastEventId: (lastEventId) => set({ lastEventId }),
  incrementFailures: () => set((s) => ({ failures: s.failures + 1 })),
  resetFailures: () => set({ failures: 0 }),
}));
