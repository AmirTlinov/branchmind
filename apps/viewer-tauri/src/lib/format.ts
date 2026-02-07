export function formatTime(ts_ms: number): string {
  if (!Number.isFinite(ts_ms)) return "—";
  const d = new Date(ts_ms);
  // Keep it compact: `2026-02-07 14:03:12`
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  const hh = String(d.getHours()).padStart(2, "0");
  const mi = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  return `${yyyy}-${mm}-${dd} ${hh}:${mi}:${ss}`;
}

export function formatRelative(ms: number): string {
  if (!Number.isFinite(ms)) return "—";
  const delta = Date.now() - ms;
  const abs = Math.abs(delta);
  const sign = delta >= 0 ? "" : "in ";
  const sec = Math.round(abs / 1000);
  if (sec < 60) return `${sign}${sec}s`;
  const min = Math.round(sec / 60);
  if (min < 60) return `${sign}${min}m`;
  const hr = Math.round(min / 60);
  if (hr < 48) return `${sign}${hr}h`;
  const day = Math.round(hr / 24);
  return `${sign}${day}d`;
}

