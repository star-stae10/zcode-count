import { ModelStat } from "../lib/api";
import { formatTokens, formatCost } from "../lib/format";

export function ModelStatsTable({ rows, summary }: {
  rows: ModelStat[];
  summary: { input_tokens: number; output_tokens: number; total_cost_usd: string; request_count: number } | null;
}) {
  const sorted = [...rows].sort((a, b) => Number(b.total_cost_usd) - Number(a.total_cost_usd));
  return (
    <div className="overflow-auto px-4 py-3">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-gray-200 text-left text-xs text-gray-500">
            <th className="py-2 pr-4 font-medium">模型</th>
            <th className="py-2 pr-4 font-medium">请求数</th>
            <th className="py-2 pr-4 font-medium">输入</th>
            <th className="py-2 pr-4 font-medium">输出</th>
            <th className="py-2 pr-4 font-medium">总成本</th>
          </tr>
        </thead>
        <tbody>
          {summary && (
            <tr className="border-b border-gray-200 bg-gray-50 font-medium">
              <td className="py-2 pr-4">所有模型</td>
              <td className="py-2 pr-4">{summary.request_count}</td>
              <td className="py-2 pr-4">{formatTokens(summary.input_tokens)}</td>
              <td className="py-2 pr-4">{formatTokens(summary.output_tokens)}</td>
              <td className="py-2 pr-4">{formatCost(summary.total_cost_usd, true)}</td>
            </tr>
          )}
          {sorted.map((r) => (
            <tr key={r.model_id} className="border-b border-gray-100">
              <td className="py-2 pr-4">{r.model_id}</td>
              <td className="py-2 pr-4">{r.request_count}</td>
              <td className="py-2 pr-4">{formatTokens(r.input_tokens)}</td>
              <td className="py-2 pr-4">{formatTokens(r.output_tokens)}</td>
              <td className="py-2 pr-4 font-medium">{formatCost(r.total_cost_usd, true)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
