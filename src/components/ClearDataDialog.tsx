import { useEffect, useState } from "react";
import {
  ClearPreview, ClearResult, ModelSel, ProviderModelRow, ScopeFilter,
  clearUsage, previewClearUsage,
} from "../lib/api";
import { dateEndExclusive, dateStart, formatDateTime } from "../lib/format";
import { ScopeGroup, ScopePicker, scopeSummary } from "./ScopePicker";

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

/** 清除表单的前端校验：返回中文错误信息，通过返回 null。 */
export function validateClearForm(useTime: boolean, sinceDate: string, untilDate: string): string | null {
  if (!useTime) return null;
  if (!sinceDate || !untilDate) return "限定时间范围时，开始与结束日期都必须填写";
  if (dateStart(sinceDate) > dateStart(untilDate)) return "开始日期晚于结束日期，请修正时间范围";
  return null;
}

/**
 * 清除用量数据弹窗（危险操作）：选择范围 → 预览将删除的数据 → 输入授权确认短语 →
 * 永久删除 → 结果反馈与审计入口。
 * 与「核算范围」的本质区别在文案中显式强调：此操作会永久删除数据。
 */
export function ClearDataDialog(props: {
  providerModels: ProviderModelRow[];
  onClose: () => void;
  onDone: () => void;        // 清除成功后关闭并刷新数据
  onOpenAudit: () => void;   // 查看审计记录
}) {
  const [useTime, setUseTime] = useState(false);
  const [sinceDate, setSinceDate] = useState("");
  const [untilDate, setUntilDate] = useState("");
  const [draft, setDraft] = useState<ScopeFilter>({ providers: [], models: [] });
  const [preview, setPreview] = useState<ClearPreview | null>(null);
  const [confirmInput, setConfirmInput] = useState("");
  const [result, setResult] = useState<ClearResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const groups = toGroups(props.providerModels);
  const timeValid = validateClearForm(useTime, sinceDate, untilDate);
  const noCriteria = !useTime && draft.providers.length === 0 && draft.models.length === 0;

  // 修改选择后，旧预览失效，必须重新预览
  function invalidatePreview() { setPreview(null); setConfirmInput(""); }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape" && !busy) props.onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [busy]);

  async function doPreview() {
    if (timeValid) { setError(timeValid); return; }
    setBusy(true); setError(null);
    try {
      const pv = await previewClearUsage({
        since: useTime && sinceDate ? dateStart(sinceDate) : null,
        until: useTime && untilDate ? dateEndExclusive(untilDate) : null,
        providers: draft.providers,
        models: draft.models,
      });
      setPreview(pv);
      setConfirmInput("");
    } catch (e) {
      setPreview(null);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function doClear() {
    if (!preview) return;
    setBusy(true); setError(null);
    try {
      const r = await clearUsage(
        {
          since: useTime && sinceDate ? dateStart(sinceDate) : null,
          until: useTime && untilDate ? dateEndExclusive(untilDate) : null,
          providers: draft.providers,
          models: draft.models,
        },
        confirmInput,
      );
      setResult(r);
      setPreview(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  function renderModelList(models: ModelSel[]) {
    if (models.length === 0) return <span className="text-gray-500">不限（所选供应商的全部模型）</span>;
    return (
      <ul className="ml-4 list-disc">
        {models.map((m) => (
          <li key={`${m.provider_id}/${m.model_id}`}>{m.provider_id} › {m.model_id}</li>
        ))}
      </ul>
    );
  }

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 p-4">
      <div className="mt-10 max-h-[85vh] w-full max-w-xl overflow-auto rounded-lg bg-white p-4 shadow-xl">
        <div className="mb-1 flex items-center justify-between">
          <h2 className="text-base font-semibold text-red-700">清除用量数据（永久删除，不可恢复）</h2>
          <button onClick={props.onClose} disabled={busy} className="rounded bg-gray-100 px-3 py-1 text-sm text-gray-700 disabled:opacity-50">关闭</button>
        </div>
        <p className="mb-3 text-xs text-red-600">
          此操作会<strong>永久删除</strong>自有库中被选范围内的用量记录，删除后不可恢复（同步也不会恢复它们）。
          它与「核算范围」不同：核算范围只影响展示，不删除数据。
        </p>

        {result ? (
          <div>
            <div className="mb-3 rounded border border-green-200 bg-green-50 p-3 text-sm text-green-800">
              <div className="font-medium">清除完成：成功删除 {result.deleted_count} 条记录。</div>
              <div className="mt-1">影响的时间范围：{result.affected_since != null && result.affected_until != null
                ? `${formatDateTime(result.affected_since)} ~ ${formatDateTime(result.affected_until)}`
                : "（未知）"}</div>
              <div>影响的供应商：{result.affected_providers.length > 0 ? result.affected_providers.join("、") : "（全部）"}</div>
              <div>影响的模型：</div>
              {renderModelList(result.affected_models)}
              <div className="mt-1">已写入审计日志（记录 #{result.audit_id}）。</div>
              {result.cursor_reset && <div>同步游标已重置（全量清除）。</div>}
            </div>
            <div className="flex gap-2">
              <button onClick={props.onOpenAudit} className="rounded bg-gray-100 px-3 py-1 text-sm text-gray-700">查看审计记录</button>
              <button onClick={props.onDone} className="rounded bg-blue-600 px-3 py-1 text-sm text-white">完成并刷新</button>
            </div>
          </div>
        ) : (
          <div>
            <label className="mb-2 flex items-center gap-2 text-sm text-gray-900">
              <input type="checkbox" checked={useTime}
                onChange={(e) => { setUseTime(e.target.checked); invalidatePreview(); }} />
              限定时间范围（不勾选 = 全部时间）
            </label>
            {useTime && (
              <div className="mb-3 flex items-center gap-2">
                <input type="date" value={sinceDate}
                  onChange={(e) => { setSinceDate(e.target.value); invalidatePreview(); }}
                  className="rounded border border-gray-300 px-2 py-1 text-sm" />
                <span className="text-xs text-gray-400">至</span>
                <input type="date" value={untilDate}
                  onChange={(e) => { setUntilDate(e.target.value); invalidatePreview(); }}
                  className="rounded border border-gray-300 px-2 py-1 text-sm" />
              </div>
            )}

            <div className="mb-1 text-xs font-medium text-gray-600">供应商与模型范围（不选 = 全部）：</div>
            <div className="mb-1 text-xs text-gray-500">已选：{scopeSummary(draft)}</div>
            <ScopePicker
              groups={groups}
              selected={draft}
              onChange={(f) => { setDraft(f); invalidatePreview(); }}
            />

            <div className="mt-3">
              <button onClick={doPreview} disabled={busy || !!timeValid}
                className="rounded bg-gray-800 px-3 py-1 text-sm text-white disabled:opacity-50">
                {busy && !preview ? "统计中…" : noCriteria ? "预览将删除的数据（全部）" : "预览将删除的数据"}
              </button>
              {timeValid && <span className="ml-2 text-xs text-red-600">{timeValid}</span>}
              {noCriteria && !timeValid && (
                <span className="ml-2 text-xs text-amber-600">未选择任何条件 = 将清除全部数据</span>
              )}
            </div>

            {preview && (
              <div className="mt-3 rounded border border-amber-300 bg-amber-50 p-3 text-sm">
                <div className="font-medium text-amber-800">
                  将删除 {preview.deleted_count} 条记录，删除后不可恢复。
                </div>
                <div className="mt-1 text-gray-700">
                  时间范围：{preview.since != null || preview.until != null
                    ? `${preview.since != null ? formatDateTime(preview.since) : "最早"} ~ ${preview.until != null ? formatDateTime(preview.until) : "最新"}`
                    : "全部时间"}
                </div>
                <div className="text-gray-700">供应商范围：{preview.providers.length > 0 ? preview.providers.join("、") : "全部供应商"}</div>
                <div className="text-gray-700">模型范围：{renderModelList(preview.models)}</div>
                <div className="mt-2 flex items-center gap-2">
                  <input
                    value={confirmInput}
                    onChange={(e) => setConfirmInput(e.target.value)}
                    placeholder={`输入「${preview.confirm_token}」以授权`}
                    className="flex-1 rounded border border-gray-300 px-2 py-1 text-sm"
                  />
                  <button onClick={doClear} disabled={busy || confirmInput !== preview.confirm_token}
                    className="rounded bg-red-600 px-3 py-1 text-sm text-white disabled:opacity-50">
                    {busy ? "删除中…" : "永久删除"}
                  </button>
                </div>
              </div>
            )}

            {error && <div className="mt-3 text-sm text-red-600">{error}</div>}
          </div>
        )}
      </div>
    </div>
  );
}
