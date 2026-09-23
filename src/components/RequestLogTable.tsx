import { RequestLogRow } from "../lib/api";
import { formatCost, formatTime } from "../lib/format";

const COLS = ["时间", "供应商", "计费模型", "输入", "输出", "总成本", "用时/首字", "状态", "来源"];

export function RequestLogTable({ rows }: { rows: RequestLogRow[] }) {
  return (
    <div className="overflow-auto px-4 py-3">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-gray-200 text-left text-xs text-gray-500">
            {COLS.map((c) => <th key={c} className="py-2 pr-4 font-medium">{c}</th>)}
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.request_id} className="border-b border-gray-100">
              <td className="py-2 pr-4 whitespace-nowrap">{formatTime(r.started_at)}</td>
              <td className="py-2 pr-4">{r.provider_id}</td>
              <td className="py-2 pr-4">{r.model_id}</td>
              <td className="py-2 pr-4">
                <div>{r.input_tokens.toLocaleString()}</div>
                <div className="text-xs text-gray-400">R{r.cache_read_tokens.toLocaleString()}</div>
              </td>
              <td className="py-2 pr-4">{r.output_tokens.toLocaleString()}</td>
              <td className="py-2 pr-4 font-medium">{formatCost(r.total_cost_usd, r.priced)}</td>
              <td className="py-2 pr-4 whitespace-nowrap text-gray-500">
                {r.duration_ms != null ? `${(r.duration_ms / 1000).toFixed(1)}s` : "—"}
                {r.first_token_ms != null ? ` / ${(r.first_token_ms / 1000).toFixed(1)}s` : ""}
              </td>
              <td className="py-2 pr-4">
                <span className={r.status === "completed" ? "text-green-600" : r.status === "error" ? "text-red-600" : "text-gray-500"}>
                  {r.status}
                </span>
              </td>
              <td className="py-2 pr-4 text-gray-500">{r.query_source ?? "—"}</td>
            </tr>
          ))}
          {rows.length === 0 && (
            <tr><td colSpan={COLS.length} className="py-6 text-center text-gray-400">暂无数据</td></tr>
          )}
        </tbody>
      </table>
    </div>
  );
}
