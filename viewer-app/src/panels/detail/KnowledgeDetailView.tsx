/* ── Knowledge Card Detail View ── */

import type { KnowledgeDetail } from "../../api/types";

interface Props { detail: KnowledgeDetail; }

export function KnowledgeDetailView({ detail }: Props) {
  const card = detail.card;
  return (
    <div className="flex flex-col gap-3 text-xs">
      <h3 className="text-sm font-semibold text-ink">{card.title || card.id}</h3>

      <div className="flex flex-wrap gap-1.5">
        <span className="rounded bg-warning/15 px-1.5 py-0.5 text-[10px] font-medium text-warning">
          {card.type}
        </span>
        <span className="rounded bg-ink-dim/15 px-1.5 py-0.5 text-[10px] text-ink-muted">
          {card.status}
        </span>
        {card.deleted && (
          <span className="rounded bg-danger/15 px-1.5 py-0.5 text-[10px] text-danger">deleted</span>
        )}
      </div>

      {card.tags && card.tags.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {card.tags.map((tag) => (
            <span key={tag} className="rounded border border-border px-1.5 py-0.5 text-[10px] text-ink-dim">
              {tag}
            </span>
          ))}
        </div>
      )}

      {card.text && (
        <div className="rounded bg-bg-raised/50 p-2 font-mono text-[11px] leading-relaxed text-ink-muted whitespace-pre-wrap">
          {card.text}
        </div>
      )}

      {detail.supports.length > 0 && (
        <div>
          <div className="mb-1 text-[10px] font-medium uppercase text-ink-dim">Supports</div>
          <div className="text-ink-muted">
            {detail.supports.map((s, i) => (
              <div key={i} className="truncate">{String(s)}</div>
            ))}
          </div>
        </div>
      )}

      {detail.blocks.length > 0 && (
        <div>
          <div className="mb-1 text-[10px] font-medium uppercase text-ink-dim">Blocks</div>
          <div className="text-ink-muted">
            {detail.blocks.map((b, i) => (
              <div key={i} className="truncate">{String(b)}</div>
            ))}
          </div>
        </div>
      )}

      <div className="border-t border-border pt-2 text-[10px] text-ink-dim">
        seq {card.last_seq} &middot; {new Date(card.last_ts_ms).toLocaleString()}
      </div>
    </div>
  );
}
