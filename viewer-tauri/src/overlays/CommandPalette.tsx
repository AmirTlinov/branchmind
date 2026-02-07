/* ── Command Palette: Ctrl+K search overlay ── */

import { useState, useEffect, useRef, useCallback } from "react";
import { useUIStore } from "../store/ui-store";
import { useSnapshotStore } from "../store/snapshot-store";
import { searchApi } from "../api/endpoints";
import type { SearchItem } from "../api/types";

export function CommandPalette() {
  const open = useUIStore((s) => s.paletteOpen);
  const setPaletteOpen = useUIStore((s) => s.setPaletteOpen);
  const openDetail = useUIStore((s) => s.openDetail);
  const project = useSnapshotStore((s) => s.project);
  const workspace = useSnapshotStore((s) => s.workspace);
  const lens = useSnapshotStore((s) => s.lens);

  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchItem[]>([]);
  const [selectedIdx, setSelectedIdx] = useState(0);
  const [searching, setSearching] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => {
    if (open) {
      setQuery("");
      setResults([]);
      setSelectedIdx(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  const doSearch = useCallback(async (q: string) => {
    if (!q.trim()) { setResults([]); return; }
    setSearching(true);
    try {
      const resp = await searchApi(project, workspace, q, lens, 20);
      setResults(resp.items ?? []);
      setSelectedIdx(0);
    } catch {
      setResults([]);
    } finally {
      setSearching(false);
    }
  }, [project, workspace, lens]);

  const handleInput = (value: string) => {
    setQuery(value);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => doSearch(value), 200);
  };

  const selectItem = (item: SearchItem) => {
    const kind = item.kind === "plan" ? "plan"
      : item.kind === "task" ? "task"
      : item.kind === "knowledge" ? "knowledge"
      : null;
    if (kind) {
      openDetail({ kind, id: item.id });
      setPaletteOpen(false);
    }
  };

  const handleKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIdx((i) => Math.min(i + 1, results.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter" && results[selectedIdx]) {
      e.preventDefault();
      selectItem(results[selectedIdx]);
    } else if (e.key === "Escape") {
      setPaletteOpen(false);
    }
  };

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[100] flex items-start justify-center pt-[20vh]">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/50" onClick={() => setPaletteOpen(false)} />

      {/* Palette */}
      <div className="glass glass-edge noise-overlay relative w-full max-w-lg rounded-xl border border-border-bright shadow-2xl">
        <div className="flex items-center gap-2 border-b border-border px-4 py-3">
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" className="text-ink-dim shrink-0">
            <circle cx="6.5" cy="6.5" r="4.5" />
            <line x1="10" y1="10" x2="14" y2="14" />
          </svg>
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => handleInput(e.target.value)}
            onKeyDown={handleKey}
            placeholder="Search plans, tasks, knowledge..."
            className="flex-1 bg-transparent text-sm text-ink outline-none placeholder:text-ink-dim"
          />
          {searching && (
            <span className="h-3 w-3 animate-spin rounded-full border border-accent/40 border-t-accent shrink-0" />
          )}
        </div>

        {results.length > 0 && (
          <div className="max-h-64 overflow-y-auto p-1">
            {results.map((item, i) => (
              <button
                key={`${item.kind}-${item.id}`}
                onClick={() => selectItem(item)}
                className={`flex w-full items-center gap-2 rounded px-3 py-2 text-left transition-colors ${
                  i === selectedIdx ? "bg-accent/10" : "hover:bg-border/50"
                }`}
              >
                <span className={`rounded px-1 py-0.5 text-[9px] font-semibold uppercase ${
                  item.kind === "plan" ? "bg-accent/15 text-accent"
                  : item.kind === "task" ? "bg-accent-2/15 text-accent-2"
                  : "bg-warning/15 text-warning"
                }`}>
                  {item.kind}
                </span>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-xs text-ink">{item.title || item.id}</div>
                  {item.snippet && (
                    <div className="truncate text-[10px] text-ink-dim mt-0.5">{item.snippet}</div>
                  )}
                </div>
              </button>
            ))}
          </div>
        )}

        {query && !searching && results.length === 0 && (
          <div className="px-4 py-6 text-center text-xs text-ink-dim">
            No results for "{query}"
          </div>
        )}
      </div>
    </div>
  );
}
