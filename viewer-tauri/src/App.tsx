/* ── Root Application Shell ── */

import { useEffect } from "react";
import { useSnapshotStore } from "./store/snapshot-store";
import { useSSE } from "./hooks/useSSE";
import { TopBar } from "./overlays/TopBar";
import { ExplorerPanel } from "./panels/ExplorerPanel";
import { DetailPanel } from "./panels/DetailPanel";
import { CommandPalette } from "./overlays/CommandPalette";
import { HUD } from "./overlays/HUD";
import { CenterArea } from "./panels/CenterArea";
import { useKeyboard } from "./hooks/useKeyboard";

export function App() {
  const boot = useSnapshotStore((s) => s.boot);
  const loading = useSnapshotStore((s) => s.loading);
  const error = useSnapshotStore((s) => s.error);
  const lens = useSnapshotStore((s) => s.lens);
  const snapshot = useSnapshotStore((s) => s.snapshot);
  const knowledgeSnapshot = useSnapshotStore((s) => s.knowledgeSnapshot);

  useEffect(() => { boot(); }, [boot]);
  useSSE();
  useKeyboard();

  const hasPrimaryData = lens === "knowledge" ? !!knowledgeSnapshot : !!snapshot;

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-bg">
      <TopBar />
      <div className="flex flex-1 overflow-hidden">
        <ExplorerPanel />
        {hasPrimaryData ? (
          <CenterArea />
        ) : (
          <main className="flex min-w-0 flex-1 items-center justify-center bg-bg">
            {loading ? (
              <div className="flex items-center gap-3 text-ink-muted">
                <span className="h-4 w-4 animate-spin rounded-full border-2 border-accent/40 border-t-accent" />
                Loading…
              </div>
            ) : error ? (
              <div className="max-w-md rounded-lg bg-danger/10 p-6 text-center text-sm text-danger">
                <p className="font-semibold mb-2">Connection Error</p>
                <p className="text-xs text-ink-muted break-words">{error}</p>
                <button
                  onClick={() => boot()}
                  className="mt-4 rounded bg-accent/20 px-3 py-1.5 text-xs text-accent hover:bg-accent/30 transition-colors"
                >
                  Retry
                </button>
                <p className="mt-3 text-[10px] text-ink-dim">
                  Tip: check the Project selector in the top bar, or start the MCP server.
                </p>
              </div>
            ) : (
              <div className="text-xs text-ink-dim">No data yet.</div>
            )}
          </main>
        )}
        <DetailPanel />
      </div>
      <HUD />
      <CommandPalette />
    </div>
  );
}
