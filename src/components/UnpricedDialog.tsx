import { UnpricedModelRow } from "../lib/api";
import { formatDateTime, formatTokens } from "../lib/format";
import { ProviderNames, providerLabel } from "../lib/providerName";

/**
 * 未定价模型清单：成本可信度警告的明细。
 * 展示供应商、模型、调用次数、token 量、影响时间段与粗略估算成本区间，
 * 并提供补充定价入口。
 */
export function UnpricedDialog(props: {
  rows: UnpricedModelRow[];
  loading: boolean;
  error: string | null;
  names: ProviderNames;
  onPriceModel: (providerId: string, modelId: string) => void;
  onClose: () => void;
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 p-4">
      <div className="mt-10 max-h-[85vh] w-full max-w-3xl overflow-auto rounded-lg bg-white p-4 shadow-xl">
        <div className="mb-1 flex items-center justify-between">
          <h2 className="text-base font-semibold text-gray-900">未定价模型清单</h2>
          <button onClick={props.onClose} className="rounded bg-gray-100 px-3 py-1 text-sm text-gray-700">关闭</button>
        </div>
        <p className="mb-3 text-xs text-amber-700">
          部分模型未配置定价，当前成本仅统计已定价模型，实际成本可能高于显示值。
          估算区间按同供应商已定价模型的每 token 单价推算，仅供参考。
        </p>

        {props.error && <div className="mb-3 text-sm text-red-600">{props.error}</div>}
        {props.loading && <div className="mb-3 text-sm text-gray-500">加载中…</div>}

        {!props.loading && !props.error && props.rows.length === 0 && (
          <div className="rounded border border-gray-200 bg-gray-50 px-3 py-4 text-sm text-gray-500">
            当前范围内所有模型均已定价，成本统计完整可信。
          </div>
        )}

        {props.rows.length > 0 && (
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-gray-200 text-left text-xs text-gray-500">
                <th className="py-2 pr-4 font-medium">供应商</th>
                <th className="py-2 pr-4 font-medium">模型</th>
                <th className="py-2 pr-4 font-medium">调用次数</th>
                <th className="py-2 pr-4 font-medium">Token 量</th>
                <th className="py-2 pr-4 font-medium">影响时间段</th>
                <th className="py-2 pr-4 font-medium">估算成本区间</th>
                <th className="py-2 pr-4 font-medium"></th>
              </tr>
            </thead>
            <tbody>
              {props.rows.map((r) => (
                <tr key={`${r.provider_id}/${r.model_id}`} className="border-b border-gray-100">
                  <td className="py-2 pr-4" title={props.names[r.provider_id] != null ? r.provider_id : undefined}>
                    {providerLabel(props.names, r.provider_id)}
                  </td>
                  <td className="py-2 pr-4">{r.model_id}</td>
                  <td className="py-2 pr-4">{r.request_count}</td>
                  <td className="py-2 pr-4">{formatTokens(r.total_tokens)}</td>
                  <td className="py-2 pr-4 text-xs text-gray-600">
                    {formatDateTime(r.first_started_at)} ~ {formatDateTime(r.last_started_at)}
                  </td>
                  <td className="py-2 pr-4 text-xs text-gray-600">
                    {r.est_cost_low_usd != null && r.est_cost_high_usd != null
                      ? `$${Number(r.est_cost_low_usd).toFixed(5)} ~ $${Number(r.est_cost_high_usd).toFixed(5)}`
                      : "无法估算（该供应商无已定价模型可参照）"}
                  </td>
                  <td className="py-2 pr-4">
                    <button
                      onClick={() => props.onPriceModel(r.provider_id, r.model_id)}
                      className="rounded bg-blue-50 px-2 py-1 text-xs text-blue-600"
                    >
                      补充定价
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
