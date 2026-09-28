import { useEffect, useState } from "react";
import { PriceOverride, listPriceOverrides, setPriceOverride, deletePriceOverride } from "../lib/api";
import { ProviderNames, providerLabel } from "../lib/providerName";

const CUSTOM = "__custom__";

/**
 * 保存结果提示：repriced = 0 说明覆盖当前没有命中任何用量记录（琥珀警示），
 * >0 显示重算条数（成功反馈）。
 */
export function SaveNotice({ repriced }: { repriced: number }) {
  if (repriced === 0) {
    return (
      <div className="mb-3 text-sm text-amber-600">
        已保存，但当前没有匹配的记录（请检查模型 ID 是否与用量记录一致）
      </div>
    );
  }
  return <div className="mb-3 text-sm text-green-600">已保存，已重算 {repriced} 条记录</div>;
}

/** 覆盖的命中记录数：0 = 覆盖未生效，红字警示。 */
export function MatchedCount({ count }: { count: number }) {
  return count === 0 ? <span className="text-red-600">{count}</span> : <span>{count}</span>;
}

/**
 * 校验四个单价：非空、有效数字、非负（允许 0）。
 * 返回中文错误信息；全部通过返回 null。
 */
export function validatePrices(input: string, output: string, cacheRead: string, cacheCreation: string): string | null {
  const fields: [string, string][] = [
    ["输入", input], ["输出", output], ["缓存读取", cacheRead], ["缓存创建", cacheCreation],
  ];
  for (const [name, raw] of fields) {
    if (raw.trim() === "") return `${name}单价不能为空`;
    const v = Number(raw);
    if (!Number.isFinite(v)) return `${name}单价不是有效数字`;
    if (v < 0) return `${name}单价不能为负`;
  }
  return null;
}

/**
 * 「高峰 = 空闲 ×2」预填规则（官方 DeepSeek 峰谷价规则，仅便利预填，可手动修改）：
 * 空闲四价均已填且为有效数字时返回翻倍结果；否则返回 null（不预填）。
 */
export function prefillPeak(
  input: string, output: string, cacheRead: string, cacheCreation: string,
): [string, string, string, string] | null {
  const vals = [input, output, cacheRead, cacheCreation].map((s) => s.trim());
  if (vals.some((s) => s === "")) return null;
  const nums = vals.map(Number);
  if (nums.some((v) => !Number.isFinite(v))) return null;
  return [String(nums[0] * 2), String(nums[1] * 2), String(nums[2] * 2), String(nums[3] * 2)];
}

/**
 * 峰谷徽标：覆盖启用高峰时段单价（四个高峰单价齐全，与后端防御口径一致）时
 * 显示在覆盖列表的模型名旁。
 */
export function PeakBadge({ row }: { row: PriceOverride }) {
  if (
    row.peak_input == null || row.peak_output == null ||
    row.peak_cache_read == null || row.peak_cache_creation == null
  ) {
    return null;
  }
  return <span className="ml-1 rounded bg-blue-50 px-1 text-xs text-blue-600">峰谷</span>;
}

export function PricingOverrideDialog(props: {
  providers: string[]; names: ProviderNames; onClose: () => void; onChanged: () => void;
  initialProvider?: string; initialModel?: string;
}) {
  const [providerSel, setProviderSel] = useState(
    props.initialProvider && props.providers.includes(props.initialProvider)
      ? props.initialProvider
      : (props.initialProvider ? CUSTOM : (props.providers[0] ?? CUSTOM)),
  );
  const [customProvider, setCustomProvider] = useState(
    props.initialProvider && !props.providers.includes(props.initialProvider) ? props.initialProvider : "",
  );
  const [modelId, setModelId] = useState(props.initialModel ?? "");
  const [input, setInput] = useState("");
  const [output, setOutput] = useState("");
  const [cacheRead, setCacheRead] = useState("");
  const [cacheCreation, setCacheCreation] = useState("");
  // 峰谷定价（DeepSeek）：展开态 + 高峰时段四价。展开且提交 = 启用；收起提交 = 关闭（peak 全不传）。
  const [peakEnabled, setPeakEnabled] = useState(false);
  const [peakInput, setPeakInput] = useState("");
  const [peakOutput, setPeakOutput] = useState("");
  const [peakCacheRead, setPeakCacheRead] = useState("");
  const [peakCacheCreation, setPeakCacheCreation] = useState("");
  const [rows, setRows] = useState<PriceOverride[]>([]);
  const [error, setError] = useState<string | null>(null);
  // 最近一次保存的重算条数（null = 尚未保存）
  const [saveNotice, setSaveNotice] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirming, setConfirming] = useState<string | null>(null);

  const providerId = providerSel === CUSTOM ? customProvider.trim() : providerSel;

  async function load() {
    try {
      setRows(await listPriceOverrides());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  useEffect(() => { void load(); }, []);

  /** 展开/收起峰谷组：展开时若峰谷价全空且空闲四价已填，按「高峰 = 空闲 ×2」预填（不覆盖已有值）。 */
  function togglePeak() {
    if (peakEnabled) {
      setPeakEnabled(false);
      return;
    }
    if (peakInput === "" && peakOutput === "" && peakCacheRead === "" && peakCacheCreation === "") {
      const pre = prefillPeak(input, output, cacheRead, cacheCreation);
      if (pre) {
        setPeakInput(pre[0]);
        setPeakOutput(pre[1]);
        setPeakCacheRead(pre[2]);
        setPeakCacheCreation(pre[3]);
      }
    }
    setPeakEnabled(true);
  }

  async function save() {
    const invalid = validatePrices(input, output, cacheRead, cacheCreation);
    if (invalid) { setError(invalid); return; }
    if (peakEnabled) {
      const peakInvalid = validatePrices(peakInput, peakOutput, peakCacheRead, peakCacheCreation);
      if (peakInvalid) { setError(`高峰时段：${peakInvalid}`); return; }
    }
    if (!providerId) { setError("请填写供应商"); return; }
    if (!modelId.trim()) { setError("请填写模型 ID"); return; }
    setSaving(true);
    try {
      // 峰谷展开时四参一起传（启用）；收起时不传（关闭峰谷，普通覆盖价保留）。
      const repriced = peakEnabled
        ? await setPriceOverride(providerId, modelId.trim(), input, output, cacheRead, cacheCreation,
            peakInput, peakOutput, peakCacheRead, peakCacheCreation)
        : await setPriceOverride(providerId, modelId.trim(), input, output, cacheRead, cacheCreation);
      setModelId(""); setInput(""); setOutput(""); setCacheRead(""); setCacheCreation("");
      setPeakEnabled(false);
      setPeakInput(""); setPeakOutput(""); setPeakCacheRead(""); setPeakCacheCreation("");
      setError(null);
      setSaveNotice(repriced);
      await load();
      props.onChanged();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }

  async function remove(pid: string, mid: string) {
    try {
      await deletePriceOverride(pid, mid);
      setError(null);
      setSaveNotice(null);
      await load();
      props.onChanged();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  const num = (v: string, set: (s: string) => void, label: string) => (
    <label className="flex flex-col gap-1 text-xs text-gray-500">
      {label}
      <input type="number" min="0" step="any" value={v}
        onChange={(e) => set(e.target.value)}
        className="rounded border border-gray-300 px-2 py-1 text-sm text-gray-900" />
    </label>
  );

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 p-4">
      <div className="mt-10 w-full max-w-2xl rounded-lg bg-white p-4 shadow-xl">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-base font-semibold text-gray-900">定价覆盖</h2>
          <button onClick={props.onClose} className="rounded bg-gray-100 px-3 py-1 text-sm text-gray-700">关闭</button>
        </div>
        <p className="mb-3 text-xs text-gray-500">
          按「供应商 + 模型」指定单价（每百万 token 的美元数），覆盖优先于 cc-switch 定价表。允许 0，不可为负。
        </p>

        <div className="mb-2 flex items-center gap-2">
          <span className="text-xs font-medium text-gray-500">空闲时段单价</span>
          <button onClick={togglePeak} className="rounded bg-blue-50 px-2 py-1 text-xs text-blue-600">
            {peakEnabled ? "关闭峰谷定价" : "适配 DeepSeek 峰谷定价"}
          </button>
        </div>
        <div className="mb-3 grid grid-cols-2 gap-2 md:grid-cols-3">
          <label className="flex flex-col gap-1 text-xs text-gray-500">
            供应商
            <select value={providerSel} onChange={(e) => setProviderSel(e.target.value)}
              className="rounded border border-gray-300 px-2 py-1 text-sm text-gray-900">
              {props.providers.map((p) => (
                <option key={p} value={p} title={props.names[p] != null ? p : undefined}>
                  {providerLabel(props.names, p)}
                </option>
              ))}
              <option value={CUSTOM}>自定义</option>
            </select>
          </label>
          {providerSel === CUSTOM && (
            <label className="flex flex-col gap-1 text-xs text-gray-500">
              自定义供应商
              <input value={customProvider} onChange={(e) => setCustomProvider(e.target.value)}
                placeholder="provider_id"
                className="rounded border border-gray-300 px-2 py-1 text-sm text-gray-900" />
            </label>
          )}
          <label className="flex flex-col gap-1 text-xs text-gray-500">
            模型 ID
            <input value={modelId} onChange={(e) => setModelId(e.target.value)}
              placeholder="model_id"
              className="rounded border border-gray-300 px-2 py-1 text-sm text-gray-900" />
          </label>
          {num(input, setInput, "输入单价")}
          {num(output, setOutput, "输出单价")}
          {num(cacheRead, setCacheRead, "缓存读取单价")}
          {num(cacheCreation, setCacheCreation, "缓存创建单价")}
        </div>

        {peakEnabled && (
          <div className="mb-3">
            <div className="mb-2 text-xs font-medium text-gray-500">高峰时段单价</div>
            <div className="grid grid-cols-2 gap-2 md:grid-cols-3">
              {num(peakInput, setPeakInput, "输入单价")}
              {num(peakOutput, setPeakOutput, "输出单价")}
              {num(peakCacheRead, setPeakCacheRead, "缓存读取单价")}
              {num(peakCacheCreation, setPeakCacheCreation, "缓存创建单价")}
            </div>
          </div>
        )}

        <div className="mb-3 flex items-center gap-2">
          <button onClick={save} disabled={saving}
            className="rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50">
            {saving ? "保存中…" : "保存"}
          </button>
        </div>

        {saveNotice !== null && <SaveNotice repriced={saveNotice} />}

        {error && <div className="mb-3 text-sm text-red-600">{error}</div>}

        <div className="max-h-60 overflow-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-gray-200 text-left text-xs text-gray-500">
                <th className="py-2 pr-4 font-medium">供应商</th>
                <th className="py-2 pr-4 font-medium">模型</th>
                <th className="py-2 pr-4 font-medium">输入</th>
                <th className="py-2 pr-4 font-medium">输出</th>
                <th className="py-2 pr-4 font-medium">缓存读取</th>
                <th className="py-2 pr-4 font-medium">缓存创建</th>
                <th className="py-2 pr-4 font-medium">命中记录数</th>
                <th className="py-2 pr-4 font-medium"></th>
              </tr>
            </thead>
            <tbody>
              {rows.length === 0 && (
                <tr><td colSpan={8} className="py-3 text-gray-500">暂无覆盖</td></tr>
              )}
              {rows.map((r) => {
                const key = `${r.provider_id}/${r.model_id}`;
                return (
                  <tr key={key} className="border-b border-gray-100">
                    <td className="py-2 pr-4" title={props.names[r.provider_id] != null ? r.provider_id : undefined}>
                      {r.provider_id ? providerLabel(props.names, r.provider_id) : "(未指定)"}
                    </td>
                    <td className="py-2 pr-4">
                      {r.model_id}
                      <PeakBadge row={r} />
                    </td>
                    <td className="py-2 pr-4">{r.input}</td>
                    <td className="py-2 pr-4">{r.output}</td>
                    <td className="py-2 pr-4">{r.cache_read}</td>
                    <td className="py-2 pr-4">{r.cache_creation}</td>
                    <td className="py-2 pr-4"><MatchedCount count={r.matched_count} /></td>
                    <td className="py-2 pr-4">
                      {confirming === key ? (
                        <span className="flex items-center gap-1">
                          <button onClick={() => { setConfirming(null); void remove(r.provider_id, r.model_id); }}
                            className="rounded bg-red-600 px-2 py-1 text-xs text-white">确认删除</button>
                          <button onClick={() => setConfirming(null)}
                            className="rounded bg-gray-100 px-2 py-1 text-xs text-gray-700">取消</button>
                        </span>
                      ) : (
                        <button onClick={() => setConfirming(key)}
                          className="rounded bg-red-50 px-2 py-1 text-xs text-red-600">删除</button>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
