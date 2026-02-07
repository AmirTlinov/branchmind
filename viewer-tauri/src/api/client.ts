/* ── HTTP client for viewer API ── */

export function qs(
  params: Record<string, string | number | boolean | undefined | null>,
): string {
  const entries = Object.entries(params).filter(
    (e): e is [string, string | number | boolean] => e[1] != null,
  );
  if (entries.length === 0) return "";
  return "?" + entries.map(([k, v]) => `${k}=${encodeURIComponent(v)}`).join("&");
}

export async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  const tauriInvoke = (window as any).__TAURI__?.core?.invoke as
    | ((cmd: string, args?: any) => Promise<any>)
    | undefined;

  if (tauriInvoke) {
    // Desktop viewer (Tauri): route via Rust backend to avoid CORS + keep API loopback-only.
    const method = (init?.method ?? "GET").toUpperCase();
    if (method === "GET" || method === "HEAD") {
      return (await tauriInvoke("viewer_api_get_json", { path: url })) as T;
    }
    if (method === "POST") {
      const body = typeof init?.body === "string" ? init.body : "";
      return (await tauriInvoke("viewer_api_post_json", { path: url, body })) as T;
    }
    throw new Error(`Unsupported viewer API method: ${method}`);
  }

  // Browser/dev fallback (best-effort).
  const res = await fetch(url, init);
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`HTTP ${res.status}: ${body.slice(0, 200)}`);
  }
  return res.json() as Promise<T>;
}
