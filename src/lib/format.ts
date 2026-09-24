export type Range = "all" | "today" | "7d" | "30d" | "custom";

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

/**
 * 成本三态显示（处理部分未定价导致的低估）：
 * - 无未定价 → `$X`
 * - 全部未定价 → `—`
 * - 部分未定价 → `≥ $X`（真实值不低于 X）
 */
export function formatCostWithUnpriced(usd: string, unpriced: number, total: number): string {
  if (unpriced > 0 && unpriced >= total) return "—";
  const base = formatCost(usd, true);
  return unpriced > 0 ? `≥ ${base}` : base;
}

export function formatTime(ms: number): string {
  const d = new Date(ms);
  const p = (x: number) => String(x).padStart(2, "0");
  return `${p(d.getMonth() + 1)}/${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

export function rangeToWindow(range: Range, customSince?: string, customUntil?: string): { since: number; until: number } {
  if (range === "custom") {
    if (!customSince || !customUntil) return { since: 0, until: Date.now() };
    // 本地日零点 → 次日零点（含结束日整天）。
    return { since: dateStart(customSince), until: dateEndExclusive(customUntil) };
  }
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

export function rangeLabel(range: Range): string {
  switch (range) {
    case "all": return "全部";
    case "today": return "当天";
    case "7d": return "7 天";
    case "30d": return "30 天";
    case "custom": return "自定义";
  }
}

/** 把 "YYYY-MM-DD" 解析为本地日零点毫秒。 */
function dateStart(s: string): number {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d).getTime();
}

/** "YYYY-MM-DD" 次日本地零点毫秒（作为含结束日的上界）。 */
function dateEndExclusive(s: string): number {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d + 1).getTime();
}
