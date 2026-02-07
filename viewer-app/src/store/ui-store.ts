/* ── UI Store (panels, detail selection, command palette) ── */

import { create } from "zustand";

export type DetailKind = "plan" | "task" | "knowledge" | "cluster";

export interface DetailSelection {
  kind: DetailKind;
  id: string;
}

export interface UIState {
  explorerOpen: boolean;
  explorerPinned: boolean;
  detailOpen: boolean;
  detailPinned: boolean;
  detailSelection: DetailSelection | null;
  paletteOpen: boolean;

  setExplorerOpen: (open: boolean) => void;
  toggleExplorer: () => void;
  setExplorerPinned: (pinned: boolean) => void;
  setDetailOpen: (open: boolean) => void;
  setDetailPinned: (pinned: boolean) => void;
  openDetail: (sel: DetailSelection) => void;
  closeDetail: () => void;
  setPaletteOpen: (open: boolean) => void;
  togglePalette: () => void;
}

export const useUIStore = create<UIState>((set) => ({
  explorerOpen: true,
  explorerPinned: true,
  detailOpen: false,
  detailPinned: false,
  detailSelection: null,
  paletteOpen: false,

  setExplorerOpen: (open) => set({ explorerOpen: open }),
  toggleExplorer: () => set((s) => ({ explorerOpen: !s.explorerOpen })),
  setExplorerPinned: (pinned) => set({ explorerPinned: pinned }),
  setDetailOpen: (open) => set({ detailOpen: open }),
  setDetailPinned: (pinned) => set({ detailPinned: pinned }),
  openDetail: (sel) => set({ detailOpen: true, detailSelection: sel }),
  closeDetail: () => set({ detailOpen: false, detailSelection: null }),
  setPaletteOpen: (open) => set({ paletteOpen: open }),
  togglePalette: () => set((s) => ({ paletteOpen: !s.paletteOpen })),
}));
