import { useCallback, useEffect, useRef } from "react";

const MIN_SCALE = 0.18;
const MAX_SCALE = 2.4;
const DEFAULT_SCALE = 0.72;

export interface ViewportResult {
  containerRef: React.RefObject<HTMLDivElement | null>;
  /** Attach to the node-layer div — transform is written directly via DOM. */
  transformRef: React.RefObject<HTMLDivElement | null>;

  /** Read-only: true while the user is actively panning the graph. */
  isPanningRef: React.RefObject<boolean>;

  viewXRef: React.RefObject<number>;
  viewYRef: React.RefObject<number>;
  scaleRef: React.RefObject<number>;
  containerSizeRef: React.RefObject<{ w: number; h: number }>;

  handleWheel: (e: React.WheelEvent) => void;
  handlePointerDown: (e: React.PointerEvent) => void;
  handlePointerMove: (e: React.PointerEvent) => void;
  handlePointerUp: (e: React.PointerEvent) => void;

  centerView: () => void;
  navigateTo: (worldX: number, worldY: number) => void;
  zoomIn: () => void;
  zoomOut: () => void;
}

/**
 * Ref-driven viewport — zero React re-renders on pan/zoom.
 *
 * All viewport state lives in refs. The node-layer DOM element is updated
 * directly via `transformRef.current.style.transform`. This eliminates the
 * biggest source of graph jank: React re-rendering 200+ nodes on every
 * mouse move.
 */
export function useViewport(): ViewportResult {
  const containerRef = useRef<HTMLDivElement>(null);
  const transformRef = useRef<HTMLDivElement>(null);

  const viewXRef = useRef(0);
  const viewYRef = useRef(0);
  const scaleRef = useRef(DEFAULT_SCALE);
  const containerSizeRef = useRef({ w: 900, h: 600 });

  const isPanningRef = useRef(false);
  const panStartRef = useRef({ x: 0, y: 0, vx: 0, vy: 0 });
  const hasCenteredRef = useRef(false);

  const setGlobalNoSelect = useCallback((enabled: boolean) => {
    if (typeof document === "undefined") return;
    document.body.classList.toggle("bm-no-select", enabled);
  }, []);

  // ── Direct DOM write ──
  const syncTransform = useCallback(() => {
    const el = transformRef.current;
    if (el) {
      el.style.transform = `translate(${viewXRef.current}px,${viewYRef.current}px) scale(${scaleRef.current})`;
    }
  }, []);

  // ── Resize observer (ref only, no state) ──
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const obs = new ResizeObserver((entries) => {
      const { width, height } = entries[0].contentRect;
      // Round to whole CSS pixels to avoid fractional-size thrash (common with
      // compositor scaling / 4K + fractional DPR). A stable container size is
      // critical: GraphCanvas uses it to size a large edges-canvas buffer.
      const w = Math.max(0, Math.round(width));
      const h = Math.max(0, Math.round(height));
      if (containerSizeRef.current.w === w && containerSizeRef.current.h === h) {
        return;
      }
      containerSizeRef.current = { w, h };
      if (!hasCenteredRef.current && w > 10 && h > 10) {
        viewXRef.current = w / 2;
        viewYRef.current = h / 2;
        hasCenteredRef.current = true;
        syncTransform();
      }
    });
    obs.observe(el);
    return () => obs.disconnect();
  }, [syncTransform]);

  // ── Handlers (all stable — zero deps on state) ──

  const handleWheel = useCallback(
    (e: React.WheelEvent) => {
      e.preventDefault();
      const rect = containerRef.current?.getBoundingClientRect();
      if (!rect) return;
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      const factor = e.deltaY > 0 ? 0.93 : 1.07;
      const prev = scaleRef.current;
      const ns = Math.max(MIN_SCALE, Math.min(MAX_SCALE, prev * factor));
      const r = ns / prev;
      viewXRef.current = mx - (mx - viewXRef.current) * r;
      viewYRef.current = my - (my - viewYRef.current) * r;
      scaleRef.current = ns;
      syncTransform();
    },
    [syncTransform],
  );

  const handlePointerDown = useCallback((e: React.PointerEvent) => {
    const tag = (e.target as HTMLElement).tagName;
    if (tag === "BUTTON" || tag === "INPUT" || tag === "TEXTAREA") return;
    if ((e.target as HTMLElement).closest("[data-no-pan]")) return;
    e.preventDefault();
    isPanningRef.current = true;
    panStartRef.current = {
      x: e.clientX,
      y: e.clientY,
      vx: viewXRef.current,
      vy: viewYRef.current,
    };
    containerRef.current?.setPointerCapture(e.pointerId);
    setGlobalNoSelect(true);
  }, [setGlobalNoSelect]);

  const handlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!isPanningRef.current) return;
      e.preventDefault();
      viewXRef.current = panStartRef.current.vx + (e.clientX - panStartRef.current.x);
      viewYRef.current = panStartRef.current.vy + (e.clientY - panStartRef.current.y);
      syncTransform();
    },
    [syncTransform],
  );

  const handlePointerUp = useCallback((e: React.PointerEvent) => {
    isPanningRef.current = false;
    setGlobalNoSelect(false);
    try {
      containerRef.current?.releasePointerCapture(e.pointerId);
    } catch {
      // ignore
    }
  }, [setGlobalNoSelect]);

  useEffect(() => {
    const stopPanning = () => {
      isPanningRef.current = false;
      setGlobalNoSelect(false);
    };
    window.addEventListener("blur", stopPanning);
    window.addEventListener("pointerup", stopPanning);
    window.addEventListener("pointercancel", stopPanning);
    return () => {
      window.removeEventListener("blur", stopPanning);
      window.removeEventListener("pointerup", stopPanning);
      window.removeEventListener("pointercancel", stopPanning);
    };
  }, [setGlobalNoSelect]);

  useEffect(() => () => setGlobalNoSelect(false), [setGlobalNoSelect]);

  // ── Navigation (all ref-driven) ──

  const centerView = useCallback(() => {
    viewXRef.current = containerSizeRef.current.w / 2;
    viewYRef.current = containerSizeRef.current.h / 2;
    scaleRef.current = DEFAULT_SCALE;
    syncTransform();
  }, [syncTransform]);

  const navigateTo = useCallback(
    (worldX: number, worldY: number) => {
      viewXRef.current = -worldX * scaleRef.current + containerSizeRef.current.w / 2;
      viewYRef.current = -worldY * scaleRef.current + containerSizeRef.current.h / 2;
      syncTransform();
    },
    [syncTransform],
  );

  const zoomToCenter = useCallback(
    (factor: number) => {
      const cx = containerSizeRef.current.w / 2;
      const cy = containerSizeRef.current.h / 2;
      const prev = scaleRef.current;
      const ns = Math.max(MIN_SCALE, Math.min(MAX_SCALE, prev * factor));
      const r = ns / prev;
      viewXRef.current = cx - (cx - viewXRef.current) * r;
      viewYRef.current = cy - (cy - viewYRef.current) * r;
      scaleRef.current = ns;
      syncTransform();
    },
    [syncTransform],
  );

  const zoomIn = useCallback(() => zoomToCenter(1.2), [zoomToCenter]);
  const zoomOut = useCallback(() => zoomToCenter(0.8), [zoomToCenter]);

  return {
    containerRef,
    transformRef,
    isPanningRef,
    viewXRef,
    viewYRef,
    scaleRef,
    containerSizeRef,
    handleWheel,
    handlePointerDown,
    handlePointerMove,
    handlePointerUp,
    centerView,
    navigateTo,
    zoomIn,
    zoomOut,
  };
}
