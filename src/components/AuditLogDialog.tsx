import { useEffect, useState } from "react";
import { AuditLogRow, listAuditLogs } from "../lib/api";
import { formatDateTime } from "../lib/format";

/** 审计日志（清除类操作的历史：操作人 / 时间 / 范围 / 删除量）。 */
export function AuditLogDialog(props: { onClose: () => void }) {
  const [rows, setRows] = useState<AuditLogRow[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listAuditLogs(100)
      .then(setRows)
      .catch((e) => setError(e instanceof Error ? e.message : String(e)));
  }, []);

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 p-4">
      <div className="mt-10 max-h-[85vh] w-full max-w-3xl overflow-auto rounded-lg bg-white p-4 shadow-xl">
        <div className="mb-1 flex items-center justify-between">
          <h2 className="text-base font-semibold text-gray-900">清除操作审计记录</h2>
          <button onClick={props.onClose} className="rounded bg-gray-100 px-3 py-1 text-sm text-gray-700">关闭</button>
        </div>
        <p className="mb-3 text-xs text-gray-500">最多显示最近 100 条清除操作记录。</p>

        {error && <div className="mb-3 text-sm text-red-600">{error}</div>}
        {rows === null && !error && <div className="mb-3 text-sm text-gray-500">加载中…</div>}

        {rows !== null && rows.length === 0 && (
          <div className="rounded border border-gray-200 bg-gray-50 px-3 py-4 text-sm text-gray-500">
            暂无审计记录（尚未执行过清除操作）。
          </div>
        )}

        {rows !== null && rows.length > 0 && (
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-gray-200 text-left text-xs text-gray-500">
                <th className="py-2 pr-4 font-medium">操作时间</th>
                <th className="py-2 pr-4 font-medium">操作人</th>
                <th className="py-2 pr-4 font-medium">操作</th>
                <th className="py-2 pr-4 font-medium">时间段</th>
                <th className="py-2 pr-4 font-medium">供应商</th>
                <th className="py-2 pr-4 font-medium">模型</th>
                <th className="py-2 pr-4 font-medium">删除条数</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.id} className="border-b border-gray-100">
                  <td className="py-2 pr-4 text-xs">{formatDateTime(r.created_at)}</td>
                  <td className="py-2 pr-4">{r.actor}</td>
                  <td className="py-2 pr-4">{r.action}</td>
                  <td className="py-2 pr-4 text-xs">
                    {r.started_at_from != null || r.started_at_to != null
                      ? `${r.started_at_from != null ? formatDateTime(r.started_at_from) : "最早"} ~ ${r.started_at_to != null ? formatDateTime(r.started_at_to) : "最新"}`
                      : "全部时间"}
                  </td>
                  <td className="py-2 pr-4 text-xs">{r.providers.length > 0 ? r.providers.join("、") : "全部"}</td>
                  <td className="py-2 pr-4 text-xs">
                    {r.models.length > 0
                      ? r.models.map((m) => `${m.provider_id} › ${m.model_id}`).join("、")
                      : "全部模型"}
                  </td>
                  <td className="py-2 pr-4">{r.deleted_count}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
