import React, { useEffect, useMemo, useState } from "react";
import { viewerApi } from "@/api/viewer";
import type { AnchorDto, GraphNodeDto, KnowledgeKeyDto } from "@/api/types";
import { cn } from "@/lib/cn";
import { formatRelative, formatTime } from "@/lib/format";
import { useStore } from "@/store";
import { Anchor, Book, RefreshCw, Search } from "lucide-react";

type Mode = "idle" | "loading" | "ready" | "error";

function ResultRow({
  active,
  title,
  meta,
  onClick,
}: {
  active: boolean;
  title: React.ReactNode;
  meta?: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "w-full px-3 py-2 rounded-lg text-left hover:bg-black/5 transition-colors",
        active && "bg-white/60 ring-1 ring-black/[0.03]",
      )}
    >
      <div className="text-[12px] text-gray-900 truncate">{title}</div>
      {meta && <div className="text-[10px] text-gray-400 truncate mt-0.5">{meta}</div>}
    </button>
  );
}

export function KnowledgeView() {
  const storage_dir = useStore((s) => s.selected_storage_dir);
  const workspace = useStore((s) => s.selected_workspace);
  const focus_card_id = useStore((s) => s.knowledge_focus_card_id);
  const set_focus_card = useStore((s) => s.set_knowledge_focus_card_id);
  const focus_anchor_id = useStore((s) => s.knowledge_focus_anchor_id);
  const set_focus_anchor = useStore((s) => s.set_knowledge_focus_anchor_id);

  const [q, setQ] = useState("");
  const [mode, setMode] = useState<Mode>("idle");
  const [err, setErr] = useState<string | null>(null);
  const [cards, setCards] = useState<KnowledgeKeyDto[]>([]);
  const [anchors, setAnchors] = useState<AnchorDto[]>([]);

  const [selectedCard, setSelectedCard] = useState<GraphNodeDto | null>(null);
  const [selectedAnchor, setSelectedAnchor] = useState<AnchorDto | null>(null);

  // Search (cards + anchors)
  useEffect(() => {
    const query = q.trim();
    if (!storage_dir || !workspace) return;
    if (query.length === 0) {
      setMode("idle");
      setErr(null);
      setCards([]);
      setAnchors([]);
      return;
    }

    const handle = window.setTimeout(async () => {
      setMode("loading");
      setErr(null);
      try {
        const [cardsRes, anchorsRes] = await Promise.all([
          viewerApi.knowledgeSearch({ storage_dir, workspace, text: query, limit: 50 }),
          viewerApi.anchorsList({ storage_dir, workspace, text: query, limit: 50 }),
        ]);
        setCards(cardsRes.items);
        setAnchors(anchorsRes.anchors);
        setMode("ready");
      } catch (e) {
        setMode("error");
        setErr(String(e));
      }
    }, 140);

    return () => window.clearTimeout(handle);
  }, [q, storage_dir, workspace]);

  // Focused card (from command palette)
  useEffect(() => {
    if (!storage_dir || !workspace) return;
    if (!focus_card_id) return;
    void (async () => {
      try {
        const node = await viewerApi.knowledgeCardGet({ storage_dir, workspace, card_id: focus_card_id });
        setSelectedCard(node);
        setSelectedAnchor(null);
      } catch {
        // ignore
      }
    })();
  }, [focus_card_id, storage_dir, workspace]);

  // Focused anchor (from command palette)
  useEffect(() => {
    if (!storage_dir || !workspace) return;
    if (!focus_anchor_id) return;
    void (async () => {
      try {
        const out = await viewerApi.anchorsList({ storage_dir, workspace, text: focus_anchor_id, limit: 50 });
        const a = out.anchors.find((x) => x.id === focus_anchor_id) ?? null;
        setSelectedAnchor(a);
        setSelectedCard(null);
      } catch {
        // ignore
      }
    })();
  }, [focus_anchor_id, storage_dir, workspace]);

  const headerHint = useMemo(() => {
    if (!storage_dir || !workspace) return "Select a workspace to browse knowledge.";
    return "Search cards & anchors. Use Cmd/Ctrl+K for global jump.";
  }, [storage_dir, workspace]);

  if (!storage_dir || !workspace) {
    return (
      <div className="w-full h-full flex items-center justify-center text-[13px] text-gray-500">
        Select a workspace to browse knowledge.
      </div>
    );
  }

  return (
    <div className="w-full h-full overflow-y-auto custom-scrollbar px-6 py-6 space-y-4">
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0">
          <div className="text-[11px] font-bold text-gray-400 uppercase tracking-widest flex items-center gap-2">
            <Book size={14} /> Knowledge
          </div>
          <div className="text-[12px] text-gray-600 mt-1 truncate">{headerHint}</div>
        </div>
        <button
          onClick={() => {
            setQ("");
            setCards([]);
            setAnchors([]);
            setErr(null);
            setMode("idle");
          }}
          className="p-2 rounded-xl bg-white/60 ring-1 ring-black/[0.03] hover:bg-white/75 transition-colors text-gray-600 shrink-0"
          title="Reset"
        >
          <RefreshCw size={14} />
        </button>
      </div>

      {/* Search */}
      <div className="bg-white/40 ring-1 ring-black/[0.03] rounded-2xl p-3 flex items-center gap-3">
        <div className="w-9 h-9 rounded-xl bg-white/60 ring-1 ring-black/[0.03] flex items-center justify-center text-gray-600 shrink-0">
          <Search size={16} />
        </div>
        <input
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Search… (e.g. determinism, viewer, graph)"
          className="flex-1 bg-transparent outline-none text-[13px] text-gray-900 placeholder:text-gray-400"
        />
      </div>

      {/* Results + detail */}
      <div className="grid grid-cols-1 lg:grid-cols-[360px_1fr] gap-4 items-start">
        <div className="space-y-4">
          <div className="bg-white/40 ring-1 ring-black/[0.03] rounded-2xl overflow-hidden">
            <div className="px-4 py-3 border-b border-gray-200/60 text-[10px] text-gray-400 uppercase tracking-widest flex items-center justify-between">
              <span>Cards</span>
              <span className="font-mono">{cards.length}</span>
            </div>
            <div className="p-2 space-y-1 max-h-[55vh] overflow-y-auto custom-scrollbar">
              {mode === "idle" && <div className="px-3 py-4 text-[12px] text-gray-500">Type to search.</div>}
              {mode === "loading" && <div className="px-3 py-4 text-[12px] text-gray-500">Searching…</div>}
              {mode === "error" && <div className="px-3 py-4 text-[12px] text-rose-600">{err}</div>}
              {mode === "ready" && cards.length === 0 && (
                <div className="px-3 py-4 text-[12px] text-gray-500">No cards.</div>
              )}
              {cards.map((c) => (
                <ResultRow
                  key={c.card_id}
                  active={c.card_id === focus_card_id}
                  title={
                    <span>
                      <span className="font-mono text-gray-500 mr-2">{c.card_id}</span>
                      <span className="text-gray-700">{c.anchor_id}</span>
                      <span className="text-gray-400"> / </span>
                      <span className="text-gray-700">{c.key}</span>
                    </span>
                  }
                  meta={`updated ${formatRelative(c.updated_at_ms)}`}
                  onClick={() => {
                    set_focus_anchor(null);
                    set_focus_card(c.card_id);
                  }}
                />
              ))}
            </div>
          </div>

          <div className="bg-white/40 ring-1 ring-black/[0.03] rounded-2xl overflow-hidden">
            <div className="px-4 py-3 border-b border-gray-200/60 text-[10px] text-gray-400 uppercase tracking-widest flex items-center justify-between">
              <span>Anchors</span>
              <span className="font-mono">{anchors.length}</span>
            </div>
            <div className="p-2 space-y-1 max-h-[55vh] overflow-y-auto custom-scrollbar">
              {mode === "ready" && anchors.length === 0 && (
                <div className="px-3 py-4 text-[12px] text-gray-500">No anchors.</div>
              )}
              {anchors.map((a) => (
                <ResultRow
                  key={a.id}
                  active={a.id === focus_anchor_id}
                  title={
                    <span>
                      <span className="font-mono text-gray-500 mr-2">{a.id}</span>
                      {a.title}
                    </span>
                  }
                  meta={`${a.kind}${a.status ? ` • ${a.status}` : ""}`}
                  onClick={() => {
                    set_focus_card(null);
                    set_focus_anchor(a.id);
                  }}
                />
              ))}
            </div>
          </div>
        </div>

        <div className="bg-white/40 ring-1 ring-black/[0.03] rounded-2xl p-5 space-y-4 min-h-[240px]">
          {!selectedCard && !selectedAnchor && (
            <div className="text-[12px] text-gray-500">Select a card or anchor to inspect.</div>
          )}

          {selectedCard && (
            <div className="space-y-3">
              <div className="flex items-start justify-between gap-4">
                <div className="min-w-0">
                  <div className="text-[10px] text-gray-400 uppercase tracking-widest">Card</div>
                  <div className="text-[16px] font-semibold text-gray-900 mt-1 truncate">
                    {selectedCard.title || selectedCard.id}
                  </div>
                  <div className="text-[11px] text-gray-500 font-mono mt-1 truncate">
                    {selectedCard.id} • {selectedCard.node_type}
                  </div>
                </div>
                <button
                  onClick={async () => {
                    try {
                      await navigator.clipboard.writeText(selectedCard.id);
                    } catch {}
                  }}
                  className="px-2 py-1 rounded-xl bg-white/60 ring-1 ring-black/[0.03] hover:bg-white/75 transition-colors text-[11px] text-gray-700"
                >
                  Copy id
                </button>
              </div>

              {selectedCard.text && (
                <pre className="text-[12px] text-gray-800 whitespace-pre-wrap leading-relaxed bg-white/55 ring-1 ring-black/[0.03] rounded-xl p-4">
                  {selectedCard.text}
                </pre>
              )}
              {!selectedCard.text && <div className="text-[12px] text-gray-500">—</div>}

              <div className="text-[10px] text-gray-400 font-mono">
                last: {formatTime(selectedCard.last_ts_ms)} • seq:{selectedCard.last_seq}
              </div>
            </div>
          )}

          {selectedAnchor && (
            <div className="space-y-3">
              <div className="flex items-start justify-between gap-4">
                <div className="min-w-0">
                  <div className="text-[10px] text-gray-400 uppercase tracking-widest flex items-center gap-2">
                    <Anchor size={12} /> Anchor
                  </div>
                  <div className="text-[16px] font-semibold text-gray-900 mt-1 truncate">
                    {selectedAnchor.title}
                  </div>
                  <div className="text-[11px] text-gray-500 font-mono mt-1 truncate">
                    {selectedAnchor.id} • {selectedAnchor.kind}
                  </div>
                </div>
                <button
                  onClick={async () => {
                    try {
                      await navigator.clipboard.writeText(selectedAnchor.id);
                    } catch {}
                  }}
                  className="px-2 py-1 rounded-xl bg-white/60 ring-1 ring-black/[0.03] hover:bg-white/75 transition-colors text-[11px] text-gray-700"
                >
                  Copy id
                </button>
              </div>

              {selectedAnchor.description && (
                <div className="text-[12px] text-gray-800 leading-relaxed">{selectedAnchor.description}</div>
              )}
              {!selectedAnchor.description && <div className="text-[12px] text-gray-500">—</div>}

              <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                <div>
                  <div className="text-[10px] text-gray-400 uppercase tracking-widest mb-1">refs</div>
                  {selectedAnchor.refs.length === 0 ? (
                    <div className="text-[12px] text-gray-500">—</div>
                  ) : (
                    <ul className="text-[12px] text-gray-800 space-y-1">
                      {selectedAnchor.refs.slice(0, 10).map((r) => (
                        <li key={r} className="font-mono break-all">{r}</li>
                      ))}
                    </ul>
                  )}
                </div>
                <div>
                  <div className="text-[10px] text-gray-400 uppercase tracking-widest mb-1">depends_on</div>
                  {selectedAnchor.depends_on.length === 0 ? (
                    <div className="text-[12px] text-gray-500">—</div>
                  ) : (
                    <ul className="text-[12px] text-gray-800 space-y-1">
                      {selectedAnchor.depends_on.slice(0, 10).map((r) => (
                        <li key={r} className="font-mono break-all">{r}</li>
                      ))}
                    </ul>
                  )}
                </div>
              </div>

              <div className="text-[10px] text-gray-400 font-mono">
                updated {formatRelative(selectedAnchor.updated_at_ms)}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

