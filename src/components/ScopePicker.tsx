import { useState } from "react";
import { ModelSel, ScopeFilter } from "../lib/api";
import { ProviderNames, providerLabel } from "../lib/providerName";

/** 层级选择器数据源：供应商及其全部模型。 */
export interface ScopeGroup {
  provider_id: string;
  models: string[];
}

/** 切换供应商级选择：勾选 = 整个供应商（其下的模型级选择移除，避免重复）；取消 = 移除。 */
export function toggleProvider(selected: ScopeFilter, provider: string): ScopeFilter {
  if (selected.providers.includes(provider)) {
    return {
      providers: selected.providers.filter((p) => p !== provider),
      models: selected.models,
    };
  }
  return {
    providers: [...selected.providers, provider],
    models: selected.models.filter((m) => m.provider_id !== provider),
  };
}

/** 切换模型级选择。若该供应商已被整体选中，先取消整体选中（改为精确到模型）。 */
export function toggleModel(selected: ScopeFilter, provider: string, model: string): ScopeFilter {
  const has = selected.models.some((m) => m.provider_id === provider && m.model_id === model);
  if (has) {
    return {
      providers: selected.providers,
      models: selected.models.filter((m) => !(m.provider_id === provider && m.model_id === model)),
    };
  }
  return {
    providers: selected.providers.filter((p) => p !== provider),
    models: [...selected.models, { provider_id: provider, model_id: model }],
  };
}

/** 某供应商下被模型级选中的模型数。 */
export function selectedModelCount(selected: ScopeFilter, provider: string): number {
  return selected.models.filter((m) => m.provider_id === provider).length;
}

/** 已选范围的人类可读摘要（供应商级与模型级分别计数）。 */
export function scopeSummary(selected: ScopeFilter): string {
  if (selected.providers.length === 0 && selected.models.length === 0) return "全部（未筛选）";
  const parts: string[] = [];
  if (selected.providers.length > 0) parts.push(`${selected.providers.length} 个供应商（全部模型）`);
  if (selected.models.length > 0) parts.push(`${selected.models.length} 个指定模型`);
  return parts.join(" + ");
}

/**
 * 供应商 > 模型 层级多选面板（核算范围与清除范围共用）。
 * 供应商级勾选 = 该供应商全部模型；也可展开后精确勾选部分模型。
 */
export function ScopePicker(props: {
  groups: ScopeGroup[];
  selected: ScopeFilter;
  onChange: (f: ScopeFilter) => void;
  names: ProviderNames;
}) {
  const { groups, selected, onChange, names } = props;
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});

  if (groups.length === 0) {
    return (
      <div className="rounded border border-gray-200 bg-gray-50 px-3 py-4 text-sm text-gray-500">
        暂无用量数据，无法选择供应商或模型。
      </div>
    );
  }

  return (
    <div className="max-h-72 overflow-auto rounded border border-gray-200">
      {groups.map((g) => {
        const providerChecked = selected.providers.includes(g.provider_id);
        const modelSel = selectedModelCount(selected, g.provider_id);
        const isExpanded = expanded[g.provider_id] ?? false;
        return (
          <div key={g.provider_id} className="border-b border-gray-100 last:border-b-0">
            <div className="flex items-center gap-2 px-3 py-2">
              <label className="flex flex-1 items-center gap-2 text-sm text-gray-900">
                <input
                  type="checkbox"
                  checked={providerChecked}
                  onChange={() => onChange(toggleProvider(selected, g.provider_id))}
                />
                {/* 显示名称（无映射回退 ID）；勾选/回调值仍是原始 provider_id */}
                <span className="font-medium" title={names[g.provider_id] != null ? g.provider_id : undefined}>
                  {providerLabel(names, g.provider_id)}
                </span>
                <span className="text-xs text-gray-400">全部模型</span>
              </label>
              {modelSel > 0 && (
                <span className="text-xs text-blue-600">{modelSel}/{g.models.length} 已选模型</span>
              )}
              <button
                type="button"
                onClick={() => setExpanded((e) => ({ ...e, [g.provider_id]: !isExpanded }))}
                className="rounded bg-gray-100 px-2 py-0.5 text-xs text-gray-600"
              >
                {isExpanded ? "收起" : `展开 ${g.models.length} 个模型`}
              </button>
            </div>
            {isExpanded && (
              <div className="grid grid-cols-1 gap-1 bg-gray-50 px-6 py-2 md:grid-cols-2">
                {g.models.map((m) => (
                  <label key={m} className="flex items-center gap-2 text-sm text-gray-700">
                    <input
                      type="checkbox"
                      checked={selected.models.some((x) => x.provider_id === g.provider_id && x.model_id === m)}
                      onChange={() => onChange(toggleModel(selected, g.provider_id, m))}
                    />
                    <span className="truncate" title={m}>{m}</span>
                  </label>
                ))}
                {g.models.length === 0 && (
                  <span className="text-xs text-gray-400">该供应商下无模型记录</span>
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

/** 已选范围 chips（含单个移除与整体清除）。显示名称，移除回调仍传原始 provider_id。 */
export function ScopeChips(props: {
  selected: ScopeFilter;
  names: ProviderNames;
  onRemoveProvider: (p: string) => void;
  onRemoveModel: (m: ModelSel) => void;
  onClear: () => void;
}) {
  const { selected, names } = props;
  if (selected.providers.length === 0 && selected.models.length === 0) return null;
  return (
    <div className="flex flex-wrap items-center gap-1">
      {selected.providers.map((p) => (
        <span key={`p-${p}`} title={names[p] != null ? p : undefined}
          className="flex items-center gap-1 rounded-full bg-blue-50 px-2 py-0.5 text-xs text-blue-700">
          {providerLabel(names, p)}（全部模型）
          <button type="button" onClick={() => props.onRemoveProvider(p)} className="text-blue-400 hover:text-blue-700" aria-label={`移除 ${providerLabel(names, p)}`}>×</button>
        </span>
      ))}
      {selected.models.map((m) => (
        <span key={`m-${m.provider_id}/${m.model_id}`} title={names[m.provider_id] != null ? m.provider_id : undefined}
          className="flex items-center gap-1 rounded-full bg-blue-50 px-2 py-0.5 text-xs text-blue-700">
          {providerLabel(names, m.provider_id)} › {m.model_id}
          <button type="button" onClick={() => props.onRemoveModel(m)} className="text-blue-400 hover:text-blue-700" aria-label={`移除 ${m.model_id}`}>×</button>
        </span>
      ))}
      <button type="button" onClick={props.onClear} className="text-xs text-gray-500 underline">
        清除筛选
      </button>
    </div>
  );
}
