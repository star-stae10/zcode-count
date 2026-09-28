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
  return `$${v.toFixed(5)}`;
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

/**
 * 成本可信度：按未定价 token 占比（未定价 token / 总 token，input+output 口径）分级。
 * - 0 未定价 → 高（成本完整可信）
 * - 占比 < 20% → 中
 * - 其余 → 低
 */
export function costConfidence(unpricedTokens: number, totalTokens: number): { label: string; level: "high" | "medium" | "low" } {
  if (unpricedTokens <= 0 || totalTokens <= 0) return { label: "高", level: "high" };
  const ratio = unpricedTokens / totalTokens;
  if (ratio < 0.2) return { label: "中（未定价 token 占比 " + (ratio * 100).toFixed(1) + "%）", level: "medium" };
  return { label: "低（未定价 token 占比 " + (ratio * 100).toFixed(1) + "%）", level: "low" };
}

/** 毫秒时间戳 → "YYYY/MM/DD HH:mm"（用于时间范围展示）。 */
export function formatDateTime(ms: number): string {
  const d = new Date(ms);
  const p = (x: number) => String(x).padStart(2, "0");
  return `${d.getFullYear()}/${p(d.getMonth() + 1)}/${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** 把 "YYYY-MM-DD" 解析为本地日零点毫秒。 */
export function dateStart(s: string): number {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d).getTime();
}

/** "YYYY-MM-DD" 次日本地零点毫秒（作为含结束日的闭区间上界）。 */
export function dateEndExclusive(s: string): number {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d + 1).getTime();
}

/** 本地日期 "YYYY-MM-DD"。 */
function todayLocal(): string {
  const d = new Date();
  const p = (x: number) => String(x).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** 导出文件默认名：时间范围编入文件名，扩展名随格式（如 zcode-count-2026-09-28.csv）。 */
export function exportFileName(
  range: Range, customSince?: string, customUntil?: string, ext: "csv" | "json" = "csv",
): string {
  const stem =
    range === "all" ? "all"
    : range === "today" ? todayLocal()
    : range === "7d" ? "7d"
    : range === "30d" ? "30d"
    : customSince && customUntil ? `${customSince}_to_${customUntil}`
    : "custom";
  return `zcode-count-${stem}.${ext}`;
}
