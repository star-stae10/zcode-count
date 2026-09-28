import { Summary } from "../lib/api";
import { costConfidence, formatTokens, formatCostWithUnpriced, Range, rangeLabel } from "../lib/format";

function Card({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-gray-200 p-3">
      <div className="text-xs text-gray-500">{label}</div>
      <div className="text-lg font-semibold text-gray-900">{value}</div>
    </div>
  );
}

export function SummaryCards({ summary, range, onShowUnpriced, onOpenPricing }: {
  summary: Summary | null;
  range: Range;
  onShowUnpriced: () => void;
  onOpenPricing: () => void;
}) {
  if (!summary) return null;
  const hasUnpriced = summary.unpriced_count > 0;
  const confidence = costConfidence(summary.unpriced_tokens, summary.input_tokens + summary.output_tokens);
  return (
    <div className="px-4 py-3">
      <div className="mb-2 text-xs text-gray-500">时间范围：{rangeLabel(range)}</div>
      {hasUnpriced && (
        <div className="mb-3 rounded border border-amber-300 bg-amber-50 p-3 text-sm text-amber-800" role="alert">
          <div className="font-medium">⚠ 部分模型未配置定价，当前成本仅统计已定价模型，实际成本可能高于显示值。</div>
          <div className="mt-1 text-xs">
            未定价模型 {summary.unpriced_models} 个，共 {summary.unpriced_count} 次调用、
            未定价 token 量 {formatTokens(summary.unpriced_tokens)}；
            成本可信度：{confidence.label}。
          </div>
          <div className="mt-2 flex gap-2">
            <button onClick={onShowUnpriced}
              className="rounded bg-amber-100 px-2 py-1 text-xs font-medium text-amber-800">
              查看未定价模型清单
            </button>
            <button onClick={onOpenPricing}
              className="rounded bg-amber-100 px-2 py-1 text-xs font-medium text-amber-800">
              配置定价
            </button>
          </div>
        </div>
      )}
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
