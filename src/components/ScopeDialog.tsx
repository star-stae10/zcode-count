import { useEffect, useState } from "react";
import { ModelSel, ProviderModelRow, ScopeFilter } from "../lib/api";
import { ProviderNames } from "../lib/providerName";
import { ScopeChips, ScopeGroup, ScopePicker, scopeSummary, toggleModel, toggleProvider } from "./ScopePicker";

function toGroups(rows: ProviderModelRow[]): ScopeGroup[] {
  const map = new Map<string, string[]>();
  for (const r of rows) {
    const list = map.get(r.provider_id);
    if (list) {
      if (!list.includes(r.model_id)) list.push(r.model_id);
    } else {
      map.set(r.provider_id, [r.model_id]);
    }
  }
  return [...map.entries()]
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([provider_id, models]) => ({ provider_id, models: models.sort() }));
}

/**
 * 成本核算范围弹窗：草稿式编辑，点「应用」生效。
 * 与清除操作的区别在文案中显式强调：仅筛选展示，不删除任何数据。
 */
export function ScopeDialog(props: {
  providerModels: ProviderModelRow[];
  names: ProviderNames;
  scope: ScopeFilter;
  onApply: (f: ScopeFilter) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState<ScopeFilter>(props.scope);
  const groups = toGroups(props.providerModels);

  // Esc 关闭：草稿不生效
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") props.onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 p-4">
      <div className="mt-10 w-full max-w-xl rounded-lg bg-white p-4 shadow-xl">
        <div className="mb-1 flex items-center justify-between">
          <h2 className="text-base font-semibold text-gray-900">成本核算范围</h2>
          <button onClick={props.onClose} className="rounded bg-gray-100 px-3 py-1 text-sm text-gray-700">关闭</button>
        </div>
        <p className="mb-3 text-xs text-gray-500">
          仅影响统计与展示，<strong>不会删除任何数据</strong>。留空 = 全部。核算范围与上方时间范围叠加生效。
        </p>

        <ScopePicker
          groups={groups}
          selected={draft}
          names={props.names}
          onChange={(f) => setDraft(f)}
        />

        <div className="mt-3 text-xs text-gray-600">已选：{scopeSummary(draft)}</div>
        <div className="mt-2">
          <ScopeChips
            selected={draft}
            names={props.names}
            onRemoveProvider={(p) => setDraft(toggleProvider(draft, p))}
            onRemoveModel={(m: ModelSel) => setDraft(toggleModel(draft, m.provider_id, m.model_id))}
            onClear={() => setDraft({ providers: [], models: [] })}
          />
        </div>

        <div className="mt-4 flex items-center gap-2">
          <button
            onClick={() => { props.onApply(draft); props.onClose(); }}
            className="rounded bg-blue-600 px-3 py-1 text-sm text-white"
          >
            应用
          </button>
          <button
            onClick={() => setDraft({ providers: [], models: [] })}
            className="rounded bg-gray-100 px-3 py-1 text-sm text-gray-700"
          >
            重置选择
          </button>
          <button
            onClick={() => { props.onApply({ providers: [], models: [] }); props.onClose(); }}
            className="rounded bg-gray-100 px-3 py-1 text-sm text-gray-700"
          >
            清除筛选并应用
          </button>
          <span className="ml-auto text-xs text-gray-400">按 Esc 取消</span>
        </div>
      </div>
    </div>
  );
}
