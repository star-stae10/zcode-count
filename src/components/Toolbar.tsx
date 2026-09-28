import { Range } from "../lib/format";
import { SyncStatus } from "../lib/api";
import { ModelSel, ScopeFilter } from "../lib/api";
import { ProviderNames } from "../lib/providerName";
import { ScopeChips, scopeSummary } from "./ScopePicker";

const RANGES: { id: Range; label: string }[] = [
  { id: "all", label: "全部" }, { id: "today", label: "当天" },
  { id: "7d", label: "7 天" }, { id: "30d", label: "30 天" },
  { id: "custom", label: "自定义" },
];

export function Toolbar(props: {
  range: Range; onRange: (r: Range) => void;
  status: SyncStatus | null; onRefresh: () => void; loading: boolean;
  scope: ScopeFilter;
  names: ProviderNames;
  onOpenScope: () => void;
  onRemoveScopeProvider: (p: string) => void;
  onRemoveScopeModel: (m: ModelSel) => void;
  onClearScope: () => void;
  customSince: string; customUntil: string;
  onCustomSince: (v: string) => void; onCustomUntil: (v: string) => void;
  onOpenPricing: () => void;
  onOpenClear: () => void;
}) {
  const scopeActive = props.scope.providers.length > 0 || props.scope.models.length > 0;
  return (
    <div className="flex flex-wrap items-center gap-3 border-b border-gray-200 px-4 py-3">
      <div className="flex gap-1">
        {RANGES.map((r) => (
          <button key={r.id}
            onClick={() => props.onRange(r.id)}
            className={`rounded px-3 py-1 text-sm ${props.range === r.id ? "bg-blue-600 text-white" : "bg-gray-100 text-gray-700"}`}>
            {r.label}
          </button>
        ))}
      </div>
      {props.range === "custom" && (
        <div className="flex items-center gap-2">
          <input type="date" value={props.customSince}
            onChange={(e) => props.onCustomSince(e.target.value)}
            className="rounded border border-gray-300 px-2 py-1 text-sm" />
          <span className="text-xs text-gray-400">至</span>
          <input type="date" value={props.customUntil}
            onChange={(e) => props.onCustomUntil(e.target.value)}
            className="rounded border border-gray-300 px-2 py-1 text-sm" />
        </div>
      )}
      {/* 核算范围：仅筛选展示，不删除数据（与「清除数据」在文案与颜色上明确区分） */}
      <button onClick={props.onOpenScope}
        className={`rounded px-3 py-1 text-sm ${scopeActive ? "bg-blue-600 text-white" : "bg-gray-100 text-gray-700"}`}>
        核算范围{scopeActive ? `（${scopeSummary(props.scope)}）` : "（全部）"}
      </button>
      <ScopeChips
        selected={props.scope}
        names={props.names}
        onRemoveProvider={props.onRemoveScopeProvider}
        onRemoveModel={props.onRemoveScopeModel}
        onClear={props.onClearScope}
      />
      <button onClick={props.onRefresh} disabled={props.loading}
        className="rounded bg-gray-800 px-3 py-1 text-sm text-white disabled:opacity-50">
        {props.loading ? "同步中…" : "刷新"}
      </button>
      <button onClick={props.onOpenPricing}
        className="rounded bg-gray-100 px-3 py-1 text-sm text-gray-700">
        定价覆盖
      </button>
      <button onClick={props.onOpenClear}
        className="rounded bg-red-600 px-3 py-1 text-sm text-white">
        清除数据
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
