import { Summary } from "../lib/api";
import { formatTokens, formatCostWithUnpriced, Range, rangeLabel } from "../lib/format";

function Card({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-gray-200 p-3">
      <div className="text-xs text-gray-500">{label}</div>
      <div className="text-lg font-semibold text-gray-900">{value}</div>
    </div>
  );
}

export function SummaryCards({ summary, range }: { summary: Summary | null; range: Range }) {
  if (!summary) return null;
  return (
    <div className="px-4 py-3">
      <div className="mb-2 text-xs text-gray-500">时间范围：{rangeLabel(range)}</div>
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4 lg:grid-cols-6">
        <Card label="总 token" value={formatTokens(summary.input_tokens + summary.output_tokens)} />
        <Card label="输入" value={formatTokens(summary.input_tokens)} />
        <Card label="输出" value={formatTokens(summary.output_tokens)} />
        <Card label="缓存读取" value={formatTokens(summary.cache_read_tokens)} />
        <Card label="缓存写" value={formatTokens(summary.cache_creation_tokens)} />
        <Card label="推理" value={formatTokens(summary.reasoning_tokens)} />
        <Card label="请求数" value={String(summary.request_count)} />
        <Card label="会话数" value={String(summary.session_count)} />
        <Card label="总成本" value={formatCostWithUnpriced(summary.total_cost_usd, summary.unpriced_count, summary.request_count)} />
      </div>
    </div>
  );
}
