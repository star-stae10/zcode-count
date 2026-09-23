import { Range } from "../lib/format";
import { SyncStatus } from "../lib/api";

const RANGES: { id: Range; label: string }[] = [
  { id: "all", label: "全部" }, { id: "today", label: "当天" },
  { id: "7d", label: "7 天" }, { id: "30d", label: "30 天" },
];

export function Toolbar(props: {
  range: Range; onRange: (r: Range) => void;
  status: SyncStatus | null; onRefresh: () => void; loading: boolean;
}) {
  return (
    <div className="flex items-center gap-3 border-b border-gray-200 px-4 py-3">
      <div className="flex gap-1">
        {RANGES.map((r) => (
          <button key={r.id}
            onClick={() => props.onRange(r.id)}
            className={`rounded px-3 py-1 text-sm ${props.range === r.id ? "bg-blue-600 text-white" : "bg-gray-100 text-gray-700"}`}>
            {r.label}
          </button>
        ))}
      </div>
      <button onClick={props.onRefresh} disabled={props.loading}
        className="rounded bg-gray-800 px-3 py-1 text-sm text-white disabled:opacity-50">
        {props.loading ? "同步中…" : "刷新"}
      </button>
      <span className="text-xs text-gray-500">
        {props.status?.last_synced_at
          ? `上次同步 ${new Date(props.status.last_synced_at).toLocaleTimeString()}`
          : "未同步"}
      </span>
      {props.status && !props.status.zcode_found && (
        <span className="text-xs text-red-600">未找到 ZCode 用量库</span>
      )}
      {props.status && props.status.zcode_found && !props.status.pricing_found && (
        <span className="text-xs text-amber-600">未找到 cc-switch 定价，成本显示 —</span>
      )}
    </div>
  );
}
