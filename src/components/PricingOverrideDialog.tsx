import { useEffect, useState } from "react";
import { PriceOverride, listPriceOverrides, setPriceOverride, deletePriceOverride } from "../lib/api";

const CUSTOM = "__custom__";

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

export function PricingOverrideDialog(props: {
  providers: string[]; onClose: () => void; onChanged: () => void;
}) {
  const [providerSel, setProviderSel] = useState(props.providers[0] ?? CUSTOM);
  const [customProvider, setCustomProvider] = useState("");
  const [modelId, setModelId] = useState("");
  const [input, setInput] = useState("");
  const [output, setOutput] = useState("");
  const [cacheRead, setCacheRead] = useState("");
  const [cacheCreation, setCacheCreation] = useState("");
  const [rows, setRows] = useState<PriceOverride[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const providerId = providerSel === CUSTOM ? customProvider.trim() : providerSel;

  async function load() {
    try {
      setRows(await listPriceOverrides());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  useEffect(() => { void load(); }, []);

  async function save() {
    const invalid = validatePrices(input, output, cacheRead, cacheCreation);
    if (invalid) { setError(invalid); return; }
    if (!providerId) { setError("请填写供应商"); return; }
    if (!modelId.trim()) { setError("请填写模型 ID"); return; }
    setSaving(true);
    try {
      await setPriceOverride(providerId, modelId.trim(), input, output, cacheRead, cacheCreation);
      setModelId(""); setInput(""); setOutput(""); setCacheRead(""); setCacheCreation("");
      setError(null);
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

        <div className="mb-3 grid grid-cols-2 gap-2 md:grid-cols-3">
          <label className="flex flex-col gap-1 text-xs text-gray-500">
            供应商
            <select value={providerSel} onChange={(e) => setProviderSel(e.target.value)}
              className="rounded border border-gray-300 px-2 py-1 text-sm text-gray-900">
              {props.providers.map((p) => <option key={p} value={p}>{p}</option>)}
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

        <div className="mb-3 flex items-center gap-2">
          <button onClick={save} disabled={saving}
            className="rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50">
            {saving ? "保存中…" : "保存"}
          </button>
        </div>

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
                <th className="py-2 pr-4 font-medium"></th>
              </tr>
            </thead>
            <tbody>
              {rows.length === 0 && (
                <tr><td colSpan={7} className="py-3 text-gray-500">暂无覆盖</td></tr>
              )}
              {rows.map((r) => (
                <tr key={`${r.provider_id}/${r.model_id}`} className="border-b border-gray-100">
                  <td className="py-2 pr-4">{r.provider_id || "（全部）"}</td>
                  <td className="py-2 pr-4">{r.model_id}</td>
                  <td className="py-2 pr-4">{r.input}</td>
                  <td className="py-2 pr-4">{r.output}</td>
                  <td className="py-2 pr-4">{r.cache_read}</td>
                  <td className="py-2 pr-4">{r.cache_creation}</td>
                  <td className="py-2 pr-4">
                    <button onClick={() => remove(r.provider_id, r.model_id)}
                      className="rounded bg-red-50 px-2 py-1 text-xs text-red-600">删除</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
