/* ── TimelinePanel: unified notes/trace feed (read-only) ── */

import { useEffect, useMemo, useRef, useState } from "react";
import { useSnapshotStore } from "../store/snapshot-store";
import { useUIStore } from "../store/ui-store";
import { getPlanDetail, getTaskDetail } from "../api/endpoints";
import type { DocEntry } from "../api/types";

type TimelineItem = {
  kind: "trace" | "notes";
  ts_ms: number;
  seq: number;
  content: string;
};

function timeLabel(ts_ms: number): string {
  const d = new Date(ts_ms);
  return d.toLocaleString(undefined, {
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function TimelinePanel() {
  const selection = useUIStore((s) => s.detailSelection);
  const snapshot = useSnapshotStore((s) => s.snapshot);
  const project = useSnapshotStore((s) => s.project);
  const workspace = useSnapshotStore((s) => s.workspace);

  const fallbackFocus =
    snapshot?.focus?.kind === "task" || snapshot?.focus?.kind === "plan"
      ? snapshot.focus
      : null;

  const target =
    selection?.kind === "task" || selection?.kind === "plan"
      ? selection
      : fallbackFocus
        ? { kind: fallbackFocus.kind, id: fallbackFocus.id! }
        : null;

  const [items, setItems] = useState<TimelineItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const tokenRef = useRef(0);

  useEffect(() => {
    if (!target) {
      setItems([]);
      setError(null);
      return;
    }

    const token = ++tokenRef.current;
    setLoading(true);
    setError(null);

    const run = async () => {
      try {
        if (target.kind === "task") {
          const detail = await getTaskDetail(target.id, project, workspace);
          if (token !== tokenRef.current) return;
          setItems(buildItems(detail.trace_tail?.entries, detail.notes_tail?.entries));
        } else {
          const detail = await getPlanDetail(target.id, project, workspace);
          if (token !== tokenRef.current) return;
          setItems(buildItems(detail.trace_tail?.entries, detail.notes_tail?.entries));
        }
      } catch (err: any) {
        if (token !== tokenRef.current) return;
        setError(err?.message || "Failed to load timeline");
        setItems([]);
      } finally {
        if (token === tokenRef.current) setLoading(false);
      }
    };

    void run();
  }, [target?.kind, target?.id, project, workspace]);

  const title = useMemo(() => {
    if (!target) return "Timeline";
    return `${target.kind.toUpperCase()} ${target.id}`;
  }, [target]);

  return (
    <section className="flex h-full w-full flex-col bg-bg">
      <div className="flex shrink-0 items-center justify-between border-b border-border px-3 py-2">
        <div>
          <div className="text-xs font-semibold text-ink">{title}</div>
          <div className="mt-0.5 text-[10px] text-ink-dim">
            Notes + trace (read-only)
          </div>
        </div>
      </div>

      {!target && (
        <div className="flex flex-1 items-center justify-center text-xs text-ink-dim">
          Select a task/plan (or focus one) to see its timeline.
        </div>
      )}

      {target && (
        <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-3">
          {loading && items.length === 0 && (
            <div className="text-xs text-ink-dim">Loading…</div>
          )}
          {error && (
            <div className="rounded border border-danger/30 bg-danger/10 p-2 text-xs text-danger">
              {error}
            </div>
          )}
          {items.map((it) => (
            <div key={`${it.kind}:${it.seq}`} className="mb-2">
              <div className="mb-1 flex items-center gap-2 text-[10px] text-ink-dim">
                <span
                  className={[
                    "rounded px-1.5 py-0.5 font-mono",
                    it.kind === "notes"
                      ? "bg-accent/15 text-accent"
                      : "bg-ink-dim/15 text-ink-dim",
                  ].join(" ")}
                >
                  {it.kind}
                </span>
                <span>{timeLabel(it.ts_ms)}</span>
                <span className="font-mono opacity-60">#{it.seq}</span>
              </div>
              <pre className="whitespace-pre-wrap rounded bg-border/20 p-2 text-[11px] leading-snug text-ink">
                {it.content.trim()}
              </pre>
            </div>
          ))}
          {items.length === 0 && !loading && !error && (
            <div className="text-xs text-ink-dim">No entries yet.</div>
          )}
        </div>
      )}
    </section>
  );
}

function buildItems(
  trace: DocEntry[] | undefined,
  notes: DocEntry[] | undefined,
): TimelineItem[] {
  const out: TimelineItem[] = [];
  for (const entry of trace ?? []) {
    out.push({
      kind: "trace",
      ts_ms: entry.ts_ms,
      seq: entry.seq,
      content: entry.content,
    });
  }
  for (const entry of notes ?? []) {
    out.push({
      kind: "notes",
      ts_ms: entry.ts_ms,
      seq: entry.seq,
      content: entry.content,
    });
  }
  out.sort((a, b) => a.ts_ms - b.ts_ms || a.seq - b.seq);
  return out;
}

