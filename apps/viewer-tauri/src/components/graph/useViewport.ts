import { useCallback, useEffect, useRef, useState } from "react";

const MIN_SCALE = 0.18;
const MAX_SCALE = 2.4;
const DEFAULT_SCALE = 0.72;

export interface ViewportResult {
  viewX: number;
  viewY: number;
  scale: number;
  containerSize: { w: number; h: number };
  containerRef: React.RefObject<HTMLDivElement | null>;

  // refs for animation frame access
  viewXRef: React.MutableRefObject<number>;
  viewYRef: React.MutableRefObject<number>;
  scaleRef: React.MutableRefObject<number>;
  containerSizeRef: React.MutableRefObject<{ w: number; h: number }>;

  // handlers
  handleWheel: (e: React.WheelEvent) => void;
  handlePointerDown: (e: React.PointerEvent) => void;
  handlePointerMove: (e: React.PointerEvent) => void;
  handlePointerUp: (e: React.PointerEvent) => void;

  // navigation
  centerView: () => void;
  navigateTo: (worldX: number, worldY: number) => void;
  zoomIn: () => void;
  zoomOut: () => void;
  setViewX: React.Dispatch<React.SetStateAction<number>>;
  setViewY: React.Dispatch<React.SetStateAction<number>>;
  setScale: React.Dispatch<React.SetStateAction<number>>;
}

export function useViewport(): ViewportResult {
  const containerRef = useRef<HTMLDivElement>(null);
  const [containerSize, setContainerSize] = useState({ w: 900, h: 600 });
  const [viewX, setViewX] = useState(0);
  const [viewY, setViewY] = useState(0);
  const [scale, setScale] = useState<number>(DEFAULT_SCALE);

  const viewXRef = useRef(viewX);
  const viewYRef = useRef(viewY);
  const scaleRef = useRef(scale);
  const containerSizeRef = useRef(containerSize);

  viewXRef.current = viewX;
  viewYRef.current = viewY;
  scaleRef.current = scale;
  containerSizeRef.current = containerSize;

  const isPanningRef = useRef(false);
  const panStartRef = useRef({ x: 0, y: 0, vx: 0, vy: 0 });

  // measure container
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const obs = new ResizeObserver((entries) => {
      const { width, height } = entries[0].contentRect;
      setContainerSize({ w: width, h: height });
    });
    obs.observe(el);
    return () => obs.disconnect();
  }, []);

  // center on mount (once)
  const hasCenteredRef = useRef(false);
  useEffect(() => {
    if (hasCenteredRef.current) return;
    if (containerSize.w > 10 && containerSize.h > 10) {
      setViewX(containerSize.w / 2);
      setViewY(containerSize.h / 2);
      hasCenteredRef.current = true;
    }
  }, [containerSize.w, containerSize.h]);

  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const factor = e.deltaY > 0 ? 0.93 : 1.07;
    setScale((prev) => {
      const newScale = Math.max(MIN_SCALE, Math.min(MAX_SCALE, prev * factor));
      const ratio = newScale / prev;
      setViewX((vx) => mx - (mx - vx) * ratio);
      setViewY((vy) => my - (my - vy) * ratio);
      return newScale;
    });
  }, []);

  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "BUTTON" || tag === "INPUT" || tag === "TEXTAREA") return;
      if ((e.target as HTMLElement).closest("[data-no-pan]")) return;
      isPanningRef.current = true;
      panStartRef.current = { x: e.clientX, y: e.clientY, vx: viewX, vy: viewY };
      containerRef.current?.setPointerCapture(e.pointerId);
    },
    [viewX, viewY],
  );

  const handlePointerMove = useCallback((e: React.PointerEvent) => {
    if (!isPanningRef.current) return;
    setViewX(panStartRef.current.vx + (e.clientX - panStartRef.current.x));
    setViewY(panStartRef.current.vy + (e.clientY - panStartRef.current.y));
  }, []);

  const handlePointerUp = useCallback((e: React.PointerEvent) => {
    isPanningRef.current = false;
    try {
      containerRef.current?.releasePointerCapture(e.pointerId);
    } catch {
      // ignore
    }
  }, []);

  const centerView = useCallback(() => {
    setViewX(containerSize.w / 2);
    setViewY(containerSize.h / 2);
    setScale(DEFAULT_SCALE);
  }, [containerSize]);

  const navigateTo = useCallback(
    (worldX: number, worldY: number) => {
      setViewX(-worldX * scale + containerSize.w / 2);
      setViewY(-worldY * scale + containerSize.h / 2);
    },
    [scale, containerSize],
  );

  const zoomToCenter = useCallback(
    (factor: number) => {
      const cx = containerSize.w / 2;
      const cy = containerSize.h / 2;
      setScale((prev) => {
        const ns = Math.max(MIN_SCALE, Math.min(MAX_SCALE, prev * factor));
        const r = ns / prev;
        setViewX((vx) => cx - (cx - vx) * r);
        setViewY((vy) => cy - (cy - vy) * r);
        return ns;
      });
    },
    [containerSize],
  );

  const zoomIn = useCallback(() => zoomToCenter(1.2), [zoomToCenter]);
  const zoomOut = useCallback(() => zoomToCenter(0.8), [zoomToCenter]);

  return {
    viewX,
    viewY,
    scale,
    containerSize,
    containerRef,
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
    setViewX,
    setViewY,
    setScale,
  };
}

