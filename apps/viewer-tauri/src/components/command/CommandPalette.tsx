import React, { useEffect, useMemo, useRef, useState } from "react";
import { viewerApi } from "@/api/viewer";
import type { AnchorDto, KnowledgeKeyDto, TaskSearchHitDto } from "@/api/types";
import { cn } from "@/lib/cn";
import { formatRelative } from "@/lib/format";
import { useStore } from "@/store";
import { Command, FileText, FolderTree, Search } from "lucide-react";

type Mode = "idle" | "loading" | "ready" | "error";

function Row({
  icon,
  title,
  meta,
  onClick,
}: {
  icon: React.ReactNode;
  title: React.ReactNode;
  meta?: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "w-full flex items-center justify-between gap-3 px-3 py-2 rounded-lg",
        "hover:bg-black/5 transition-colors text-left",
      )}
    >
      <div className="flex items-center gap-2.5 min-w-0">
        <div className="w-6 h-6 rounded-md bg-white/60 ring-1 ring-black/[0.03] flex items-center justify-center text-gray-600 shrink-0">
          {icon}
        </div>
        <div className="min-w-0">
          <div className="text-[12px] text-gray-900 truncate">{title}</div>
          {meta && <div className="text-[10px] text-gray-400 truncate">{meta}</div>}
        </div>
      </div>
      <div className="text-[10px] text-gray-300 font-mono shrink-0">↵</div>
    </button>
  );
}

export function CommandPalette() {
  const open = useStore((s) => s.command_palette_open);
  const set_open = useStore((s) => s.set_command_palette_open);
  const set_active_view = useStore((s) => s.set_active_view);
  const select_task = useStore((s) => s.select_task);
  const set_focus_card = useStore((s) => s.set_knowledge_focus_card_id);
  const set_focus_anchor = useStore((s) => s.set_knowledge_focus_anchor_id);

  const storage_dir = useStore((s) => s.selected_storage_dir);
  const workspace = useStore((s) => s.selected_workspace);

  const inputRef = useRef<HTMLInputElement | null>(null);
  const [q, setQ] = useState("");
  const [mode, setMode] = useState<Mode>("idle");
  const [err, setErr] = useState<string | null>(null);
  const [tasks, setTasks] = useState<TaskSearchHitDto[]>([]);
  const [cards, setCards] = useState<KnowledgeKeyDto[]>([]);
  const [anchors, setAnchors] = useState<AnchorDto[]>([]);

  // Global shortcut
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        set_open(!open);
        return;
      }
      if (open && e.key === "Escape") {
        e.preventDefault();
        set_open(false);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, set_open]);

  // Focus on open
  useEffect(() => {
    if (!open) return;
    setQ("");
    setMode("idle");
    setErr(null);
    setTasks([]);
    setCards([]);
    setAnchors([]);
    const t = window.setTimeout(() => inputRef.current?.focus(), 50);
    return () => window.clearTimeout(t);
  }, [open]);

  // Search
  useEffect(() => {
    if (!open) return;
    const query = q.trim();
    if (query.length === 0) {
      setMode("idle");
      setErr(null);
      setTasks([]);
      setCards([]);
      setAnchors([]);
      return;
    }

    const handle = window.setTimeout(async () => {
      if (!storage_dir || !workspace) {
        setMode("error");
        setErr("Select a workspace first.");
        return;
      }

      setMode("loading");
      setErr(null);
      try {
        const [tasksRes, cardsRes, anchorsRes] = await Promise.all([
          viewerApi.tasksSearch({ storage_dir, workspace, text: query, limit: 20 }),
          viewerApi.knowledgeSearch({ storage_dir, workspace, text: query, limit: 20 }),
          viewerApi.anchorsList({ storage_dir, workspace, text: query, limit: 20 }),
        ]);
        setTasks(tasksRes.tasks);
        setCards(cardsRes.items);
        setAnchors(anchorsRes.anchors);
        setMode("ready");
      } catch (e) {
        setMode("error");
        setErr(String(e));
      }
    }, 140);

    return () => window.clearTimeout(handle);
  }, [open, q, storage_dir, workspace]);

  const isMac = useMemo(() => typeof navigator !== "undefined" && /Mac/.test(navigator.platform), []);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/10 backdrop-blur-[2px] flex items-start justify-center pt-20"
      onMouseDown={() => set_open(false)}
    >
      <div
        className="w-[740px] max-w-[92vw] rounded-2xl bg-white/70 backdrop-blur-xl ring-1 ring-black/[0.06] shadow-xl shadow-black/10 overflow-hidden"
        onMouseDown={(e) => e.stopPropagation()}
      >
        {/* Search bar */}
        <div className="px-4 py-3 border-b border-gray-200/60 flex items-center gap-3">
          <div className="w-9 h-9 rounded-xl bg-white/60 ring-1 ring-black/[0.03] flex items-center justify-center text-gray-600">
            <Search size={16} />
          </div>
          <input
            ref={inputRef}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="Search tasks, cards, anchors…"
            className="flex-1 bg-transparent outline-none text-[13px] text-gray-900 placeholder:text-gray-400"
          />
          <div className="text-[10px] text-gray-400 font-mono flex items-center gap-1">
            <Command size={12} />
            <span>{isMac ? "K" : "K"}</span>
          </div>
        </div>

        {/* Results */}
        <div className="max-h-[60vh] overflow-y-auto custom-scrollbar p-2">
          {mode === "idle" && (
            <div className="px-3 py-6 text-[12px] text-gray-500">
              Type to search. <span className="text-gray-400">(Esc to close)</span>
            </div>
          )}
          {mode === "loading" && (
            <div className="px-3 py-6 text-[12px] text-gray-500">Searching…</div>
          )}
          {mode === "error" && (
            <div className="px-3 py-6 text-[12px] text-rose-600">{err}</div>
          )}

          {mode === "ready" && tasks.length === 0 && cards.length === 0 && anchors.length === 0 && (
            <div className="px-3 py-6 text-[12px] text-gray-500">No matches.</div>
          )}

          {tasks.length > 0 && (
            <div className="mb-3">
              <div className="px-3 py-2 text-[10px] text-gray-400 uppercase tracking-widest">
                Tasks
              </div>
              <div className="space-y-1">
                {tasks.map((t) => (
                  <Row
                    key={t.id}
                    icon={<FolderTree size={14} />}
                    title={
                      <span>
                        <span className="font-mono text-gray-500 mr-2">{t.id}</span>
                        {t.title}
                      </span>
                    }
                    meta={`updated ${formatRelative(t.updated_at_ms)} • ${t.plan_id}`}
                    onClick={() => {
                      set_open(false);
                      set_active_view("plan");
                      void select_task(t.id);
                    }}
                  />
                ))}
              </div>
            </div>
          )}

          {cards.length > 0 && (
            <div className="mb-3">
              <div className="px-3 py-2 text-[10px] text-gray-400 uppercase tracking-widest">
                Knowledge cards
              </div>
              <div className="space-y-1">
                {cards.map((c) => (
                  <Row
                    key={c.card_id}
                    icon={<FileText size={14} />}
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
                      set_open(false);
                      set_active_view("knowledge");
                      set_focus_anchor(null);
                      set_focus_card(c.card_id);
                    }}
                  />
                ))}
              </div>
            </div>
          )}

          {anchors.length > 0 && (
            <div className="mb-1">
              <div className="px-3 py-2 text-[10px] text-gray-400 uppercase tracking-widest">
                Anchors
              </div>
              <div className="space-y-1">
                {anchors.map((a) => (
                  <Row
                    key={a.id}
                    icon={<FolderTree size={14} />}
                    title={
                      <span>
                        <span className="font-mono text-gray-500 mr-2">{a.id}</span>
                        {a.title}
                      </span>
                    }
                    meta={a.kind}
                    onClick={() => {
                      set_open(false);
                      set_active_view("knowledge");
                      set_focus_card(null);
                      set_focus_anchor(a.id);
                    }}
                  />
                ))}
              </div>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="px-4 py-2 border-t border-gray-200/60 flex items-center justify-between text-[10px] text-gray-400">
          <div>
            {storage_dir && workspace ? (
              <span className="font-mono">
                {workspace}
              </span>
            ) : (
              <span>Select a workspace to enable search.</span>
            )}
          </div>
          <div className="font-mono">Esc</div>
        </div>
      </div>
    </div>
  );
}

