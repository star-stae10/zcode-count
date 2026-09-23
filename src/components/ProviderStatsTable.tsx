import { ProviderStat } from "../lib/api";
import { formatTokens, formatCost } from "../lib/format";

export function ProviderStatsTable({ rows }: { rows: ProviderStat[] }) {
  return (
    <div className="overflow-auto px-4 py-3">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-gray-200 text-left text-xs text-gray-500">
            <th className="py-2 pr-4 font-medium">供应商</th>
            <th className="py-2 pr-4 font-medium">请求数</th>
            <th className="py-2 pr-4 font-medium">输入</th>
            <th className="py-2 pr-4 font-medium">输出</th>
            <th className="py-2 pr-4 font-medium">总成本</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.provider_id} className="border-b border-gray-100">
              <td className="py-2 pr-4">{r.provider_id}</td>
              <td className="py-2 pr-4">{r.request_count}</td>
              <td className="py-2 pr-4">{formatTokens(r.input_tokens)}</td>
              <td className="py-2 pr-4">{formatTokens(r.output_tokens)}</td>
              <td className="py-2 pr-4 font-medium">{formatCost(r.total_cost_usd, r.unpriced_count < r.request_count)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
