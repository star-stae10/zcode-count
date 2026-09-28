import { useEffect, useRef, useState } from "react";
import {
  syncUsage, getSummary, listLogs, getProviderStats, getModelStats,
  listProviderModels, getUnpricedModels,
  Summary, SyncStatus, RequestLogRow, ProviderStat, ModelStat,
  ProviderModelRow, UnpricedModelRow, ScopeFilter,
} from "./lib/api";
import { Range, rangeToWindow } from "./lib/format";
import { Toolbar } from "./components/Toolbar";
import { SummaryCards } from "./components/SummaryCards";
import { Tabs, TabId } from "./components/Tabs";
import { RequestLogTable } from "./components/RequestLogTable";
import { ProviderStatsTable } from "./components/ProviderStatsTable";
import { ModelStatsTable } from "./components/ModelStatsTable";
import { PricingOverrideDialog } from "./components/PricingOverrideDialog";
import { ScopeDialog } from "./components/ScopeDialog";
import { ClearDataDialog } from "./components/ClearDataDialog";
import { UnpricedDialog } from "./components/UnpricedDialog";
import { AuditLogDialog } from "./components/AuditLogDialog";
import { toggleModel, toggleProvider } from "./components/ScopePicker";

const EMPTY_SCOPE: ScopeFilter = { providers: [], models: [] };

function todayInput(): string {
  const d = new Date();
  const p = (x: number) => String(x).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export default function App() {
  const [range, setRange] = useState<Range>("all");
  // 核算范围（供应商 > 模型层级多选）：仅影响统计与展示，不删除数据。
  const [scope, setScope] = useState<ScopeFilter>(EMPTY_SCOPE);
  const [customSince, setCustomSince] = useState(todayInput());
  const [customUntil, setCustomUntil] = useState(todayInput());
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [tab, setTab] = useState<TabId>("logs");
  const [logs, setLogs] = useState<RequestLogRow[]>([]);
  const [providerStats, setProviderStats] = useState<ProviderStat[]>([]);
  const [providerOptions, setProviderOptions] = useState<string[]>([]);
  const [providerModels, setProviderModels] = useState<ProviderModelRow[]>([]);
  const [modelStats, setModelStats] = useState<ModelStat[]>([]);
  const [unpricedRows, setUnpricedRows] = useState<UnpricedModelRow[]>([]);
  const [unpricedLoading, setUnpricedLoading] = useState(false);
  const [unpricedError, setUnpricedError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pricingOpen, setPricingOpen] = useState(false);
  const [pricingInitial, setPricingInitial] = useState<{ provider?: string; model?: string }>({});
  const [scopeOpen, setScopeOpen] = useState(false);
  const [clearOpen, setClearOpen] = useState(false);
  const [auditOpen, setAuditOpen] = useState(false);
  const [unpricedOpen, setUnpricedOpen] = useState(false);
  // 请求序号：防止快速切换筛选时旧响应覆盖新状态
  const refreshSeq = useRef(0);

  async function refresh() {
    const seq = ++refreshSeq.current;
    setLoading(true);
    try {
      const st = await syncUsage();
      if (seq !== refreshSeq.current) return;
      setStatus(st);
      const { since, until } = rangeToWindow(range, customSince, customUntil);
      const scopeParam = scope.providers.length > 0 || scope.models.length > 0 ? scope : null;
      const [sum, logRows, provRows, modelRows, unpr, allProvRows, pmRows] = await Promise.all([
        getSummary(since, until, scopeParam),
        listLogs(since, until, scopeParam, 500),
        getProviderStats(since, until, scopeParam),
        getModelStats(since, until, scopeParam),
        getUnpricedModels(since, until, scopeParam),
        // 核算范围与定价弹窗的选项始终取未筛选的全量列表，筛选后仍可切换
        scopeParam
          ? getProviderStats(since, until, null)
          : Promise.resolve(null as ProviderStat[] | null),
        listProviderModels(),
      ]);
      if (seq !== refreshSeq.current) return;
      setSummary(sum);
      setLogs(logRows);
      setProviderStats(provRows);
      setModelStats(modelRows);
      setUnpricedRows(unpr);
      setProviderOptions((allProvRows ?? provRows).map((p) => p.provider_id));
      setProviderModels(pmRows);
      setError(null);
    } catch (e) {
      if (seq === refreshSeq.current) {
        setError(e instanceof Error ? e.message : String(e));
      }
    } finally {
      if (seq === refreshSeq.current) setLoading(false);
    }
  }

  // 打开未定价清单时重新拉取，保证弹窗内数据新鲜（含独立加载/错误状态）
  async function fetchUnpriced() {
    setUnpricedLoading(true);
    setUnpricedError(null);
    try {
      const { since, until } = rangeToWindow(range, customSince, customUntil);
      const scopeParam = scope.providers.length > 0 || scope.models.length > 0 ? scope : null;
      setUnpricedRows(await getUnpricedModels(since, until, scopeParam));
    } catch (e) {
      setUnpricedError(e instanceof Error ? e.message : String(e));
    } finally {
      setUnpricedLoading(false);
    }
  }

  useEffect(() => { void refresh(); }, [range, scope, customSince, customUntil]);

  function openUnpriced() {
    setUnpricedOpen(true);
    void fetchUnpriced();
  }

  function priceUnpricedModel(providerId: string, modelId: string) {
    setUnpricedOpen(false);
    setPricingInitial({ provider: providerId, model: modelId });
    setPricingOpen(true);
  }

  return (
    <div className="min-h-screen bg-gray-50 text-gray-900">
      <Toolbar
        range={range} onRange={setRange}
        status={status} onRefresh={refresh} loading={loading}
        scope={scope}
        onOpenScope={() => setScopeOpen(true)}
        onRemoveScopeProvider={(p) => setScope(toggleProvider(scope, p))}
        onRemoveScopeModel={(m) => setScope(toggleModel(scope, m.provider_id, m.model_id))}
        onClearScope={() => setScope(EMPTY_SCOPE)}
        customSince={customSince} customUntil={customUntil}
        onCustomSince={setCustomSince} onCustomUntil={setCustomUntil}
        onOpenPricing={() => { setPricingInitial({}); setPricingOpen(true); }}
        onOpenClear={() => setClearOpen(true)}
      />
      {scopeOpen && (
        <ScopeDialog
          providerModels={providerModels}
          scope={scope}
          onApply={setScope}
          onClose={() => setScopeOpen(false)}
        />
      )}
      {clearOpen && (
        <ClearDataDialog
          providerModels={providerModels}
          onClose={() => setClearOpen(false)}
          onDone={() => { setClearOpen(false); void refresh(); }}
          onOpenAudit={() => { setClearOpen(false); setAuditOpen(true); }}
        />
      )}
      {auditOpen && <AuditLogDialog onClose={() => setAuditOpen(false)} />}
      {unpricedOpen && (
        <UnpricedDialog
          rows={unpricedRows}
          loading={unpricedLoading}
          error={unpricedError}
          onPriceModel={priceUnpricedModel}
          onClose={() => setUnpricedOpen(false)}
        />
      )}
      {pricingOpen && (
        <PricingOverrideDialog
          providers={providerOptions}
          initialProvider={pricingInitial.provider}
          initialModel={pricingInitial.model}
          onClose={() => setPricingOpen(false)}
          onChanged={() => void refresh()}
        />
      )}
      {error && <div className="px-4 pt-3 text-sm text-red-600">{error}</div>}
      <SummaryCards
        summary={summary} range={range}
        onShowUnpriced={openUnpriced}
        onOpenPricing={() => { setPricingInitial({}); setPricingOpen(true); }}
      />
      <Tabs active={tab} onChange={setTab} />
      {tab === "logs" && <RequestLogTable rows={logs} />}
      {tab === "providers" && <ProviderStatsTable rows={providerStats} />}
      {tab === "models" && <ModelStatsTable rows={modelStats} summary={summary} />}
    </div>
  );
}
