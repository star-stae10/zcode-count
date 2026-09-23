export type Range = "all" | "today" | "7d" | "30d";

export function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

export function formatCost(usd: string, priced: boolean): string {
  if (!priced) return "—";
  const v = Number(usd);
  if (!Number.isFinite(v)) return "—";
  return `$${v.toFixed(4)}`;
}

export function formatTime(ms: number): string {
  const d = new Date(ms);
  const p = (x: number) => String(x).padStart(2, "0");
  return `${p(d.getMonth() + 1)}/${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

export function rangeToWindow(range: Range): { since: number; until: number } {
  const until = Date.now();
  if (range === "all") return { since: 0, until };
  const now = new Date();
  let since: number;
  if (range === "today") {
    since = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  } else {
    const days = range === "7d" ? 7 : 30;
    since = until - days * 24 * 60 * 60 * 1000;
  }
  return { since, until };
}
