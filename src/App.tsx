import { useEffect, useState } from "react";
import { syncUsage, getSummary, listLogs, getProviderStats, getModelStats, Summary, SyncStatus, RequestLogRow, ProviderStat, ModelStat } from "./lib/api";
import { Range, rangeToWindow } from "./lib/format";
import { Toolbar } from "./components/Toolbar";
import { SummaryCards } from "./components/SummaryCards";
import { Tabs, TabId } from "./components/Tabs";
import { RequestLogTable } from "./components/RequestLogTable";
import { ProviderStatsTable } from "./components/ProviderStatsTable";
import { ModelStatsTable } from "./components/ModelStatsTable";
import { PricingOverrideDialog } from "./components/PricingOverrideDialog";

function todayInput(): string {
  const d = new Date();
  const p = (x: number) => String(x).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export default function App() {
  const [range, setRange] = useState<Range>("all");
  const [provider, setProvider] = useState<string | null>(null);
  const [customSince, setCustomSince] = useState(todayInput());
  const [customUntil, setCustomUntil] = useState(todayInput());
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [tab, setTab] = useState<TabId>("logs");
  const [logs, setLogs] = useState<RequestLogRow[]>([]);
  const [providerStats, setProviderStats] = useState<ProviderStat[]>([]);
  const [providerOptions, setProviderOptions] = useState<string[]>([]);
  const [modelStats, setModelStats] = useState<ModelStat[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pricingOpen, setPricingOpen] = useState(false);

  async function refresh() {
    setLoading(true);
    try {
      const st = await syncUsage();
      setStatus(st);
      const { since, until } = rangeToWindow(range, customSince, customUntil);
      setSummary(await getSummary(since, until, provider));
      setLogs(await listLogs(since, until, provider, 500));
      const providerRows = await getProviderStats(since, until, provider);
      setProviderStats(providerRows);
      setModelStats(await getModelStats(since, until, provider));
      // 供应商下拉与定价弹窗的选项始终取未筛选的全量列表，筛选后仍可切换供应商
      const allProviderStats = provider === null
        ? providerRows
        : await getProviderStats(since, until, null);
      setProviderOptions(allProviderStats.map((p) => p.provider_id));
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void refresh(); }, [range, provider, customSince, customUntil]);

  return (
    <div className="min-h-screen bg-gray-50 text-gray-900">
      <Toolbar
        range={range} onRange={setRange}
        status={status} onRefresh={refresh} loading={loading}
        provider={provider} onProvider={setProvider}
        providers={providerOptions}
        customSince={customSince} customUntil={customUntil}
        onCustomSince={setCustomSince} onCustomUntil={setCustomUntil}
        onOpenPricing={() => setPricingOpen(true)}
      />
      {pricingOpen && (
        <PricingOverrideDialog
          providers={providerOptions}
          onClose={() => setPricingOpen(false)}
          onChanged={() => void refresh()}
        />
      )}
      {error && <div className="px-4 pt-3 text-sm text-red-600">{error}</div>}
      <SummaryCards summary={summary} range={range} />
      <Tabs active={tab} onChange={setTab} />
      {tab === "logs" && <RequestLogTable rows={logs} />}
      {tab === "providers" && <ProviderStatsTable rows={providerStats} />}
      {tab === "models" && <ModelStatsTable rows={modelStats} summary={summary} />}
    </div>
  );
}
