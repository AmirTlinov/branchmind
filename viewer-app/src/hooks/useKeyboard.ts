/* ── Global Keyboard Shortcuts ── */

import { useEffect } from "react";
import { useUIStore } from "../store/ui-store";

export function useKeyboard() {
  const togglePalette = useUIStore((s) => s.togglePalette);
  const toggleExplorer = useUIStore((s) => s.toggleExplorer);
  const closeDetail = useUIStore((s) => s.closeDetail);
  const setPaletteOpen = useUIStore((s) => s.setPaletteOpen);

  useEffect(() => {
    function handler(e: KeyboardEvent) {
      // Ctrl+K or Cmd+K → command palette
      if ((e.ctrlKey || e.metaKey) && e.key === "k") {
        e.preventDefault();
        togglePalette();
        return;
      }
      // Ctrl+B → toggle explorer
      if ((e.ctrlKey || e.metaKey) && e.key === "b") {
        e.preventDefault();
        toggleExplorer();
        return;
      }
      // Escape → close palette or detail
      if (e.key === "Escape") {
        const state = useUIStore.getState();
        if (state.paletteOpen) {
          setPaletteOpen(false);
        } else if (state.detailOpen) {
          closeDetail();
        }
        return;
      }
    }

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [togglePalette, toggleExplorer, closeDetail, setPaletteOpen]);
}
