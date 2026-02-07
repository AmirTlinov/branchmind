import React, { useCallback, useEffect, useRef } from "react";
import type { SimNode } from "./useForceLayout";

const MINI_W = 180;
const MINI_H = 120;

function computeBounds(nodes: SimNode[]) {
  if (nodes.length === 0) return { minX: -300, maxX: 300, minY: -220, maxY: 220 };
  let minX = Infinity;
  let maxX = -Infinity;
  let minY = Infinity;
  let maxY = -Infinity;
  for (const n of nodes) {
    minX = Math.min(minX, n._x);
    maxX = Math.max(maxX, n._x);
    minY = Math.min(minY, n._y);
    maxY = Math.max(maxY, n._y);
  }
  const padX = Math.max((maxX - minX) * 0.12, 120);
  const padY = Math.max((maxY - minY) * 0.12, 90);
  return { minX: minX - padX, maxX: maxX + padX, minY: minY - padY, maxY: maxY + padY };
}

export function Minimap({
  simNodes,
  viewXRef,
  viewYRef,
  scaleRef,
  containerSizeRef,
  selectedId,
  navigateTo,
}: {
  simNodes: SimNode[];
  viewXRef: React.MutableRefObject<number>;
  viewYRef: React.MutableRefObject<number>;
  scaleRef: React.MutableRefObject<number>;
  containerSizeRef: React.MutableRefObject<{ w: number; h: number }>;
  selectedId: string | null;
  navigateTo: (worldX: number, worldY: number) => void;
}) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== MINI_W * dpr || canvas.height !== MINI_H * dpr) {
      canvas.width = MINI_W * dpr;
      canvas.height = MINI_H * dpr;
      canvas.style.width = `${MINI_W}px`;
      canvas.style.height = `${MINI_H}px`;
    }
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, MINI_W, MINI_H);

    // Backplate
    ctx.fillStyle = "rgba(255,255,255,0.55)";
    ctx.strokeStyle = "rgba(0,0,0,0.06)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.roundRect(0, 0, MINI_W, MINI_H, 12);
    ctx.fill();
    ctx.stroke();

    const b = computeBounds(simNodes);
    const spanX = Math.max(b.maxX - b.minX, 1);
    const spanY = Math.max(b.maxY - b.minY, 1);

    const toMini = (wx: number, wy: number) => ({
      x: ((wx - b.minX) / spanX) * (MINI_W - 16) + 8,
      y: ((wy - b.minY) / spanY) * (MINI_H - 16) + 8,
    });

    // Nodes
    for (const n of simNodes) {
      const p = toMini(n._x, n._y);
      ctx.beginPath();
      ctx.arc(p.x, p.y, n.id === selectedId ? 2.6 : 1.6, 0, Math.PI * 2);
      ctx.fillStyle = n.id === selectedId ? "rgba(17,24,39,0.75)" : "rgba(17,24,39,0.25)";
      ctx.fill();
    }

    // Viewport rectangle
    const scale = scaleRef.current;
    const viewX = viewXRef.current;
    const viewY = viewYRef.current;
    const { w: cw, h: ch } = containerSizeRef.current;
    const leftW = (0 - viewX) / scale;
    const rightW = (cw - viewX) / scale;
    const topW = (0 - viewY) / scale;
    const bottomW = (ch - viewY) / scale;

    const p1 = toMini(leftW, topW);
    const p2 = toMini(rightW, bottomW);
    const rx = Math.min(p1.x, p2.x);
    const ry = Math.min(p1.y, p2.y);
    const rw = Math.max(6, Math.abs(p2.x - p1.x));
    const rh = Math.max(6, Math.abs(p2.y - p1.y));
    ctx.strokeStyle = "rgba(17,24,39,0.35)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.roundRect(rx, ry, rw, rh, 6);
    ctx.stroke();
  }, [simNodes, selectedId, containerSizeRef, navigateTo, scaleRef, viewXRef, viewYRef]);

  // rAF loop (throttled)
  useEffect(() => {
    let raf = 0;
    let last = 0;
    const loop = (t: number) => {
      if (t - last > 80) {
        last = t;
        draw();
      }
      raf = window.requestAnimationFrame(loop);
    };
    raf = window.requestAnimationFrame(loop);
    return () => window.cancelAnimationFrame(raf);
  }, [draw]);

  const handleClick = useCallback(
    (e: React.MouseEvent) => {
      const rect = canvasRef.current?.getBoundingClientRect();
      if (!rect) return;
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      const b = computeBounds(simNodes);
      const spanX = Math.max(b.maxX - b.minX, 1);
      const spanY = Math.max(b.maxY - b.minY, 1);
      const wx = ((x - 8) / (MINI_W - 16)) * spanX + b.minX;
      const wy = ((y - 8) / (MINI_H - 16)) * spanY + b.minY;
      navigateTo(wx, wy);
    },
    [navigateTo, simNodes],
  );

  return (
    <div className="absolute bottom-4 right-4 z-30" data-no-pan>
      <canvas
        ref={canvasRef}
        className="rounded-xl"
        width={MINI_W}
        height={MINI_H}
        onMouseDown={(e) => e.stopPropagation()}
        onClick={handleClick}
      />
    </div>
  );
}
