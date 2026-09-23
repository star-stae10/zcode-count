import { useEffect, useState } from "react";
import { syncUsage, getSummary, listLogs, Summary, SyncStatus, RequestLogRow } from "./lib/api";
import { Range, rangeToWindow } from "./lib/format";
import { Toolbar } from "./components/Toolbar";
import { SummaryCards } from "./components/SummaryCards";
import { Tabs, TabId } from "./components/Tabs";
import { RequestLogTable } from "./components/RequestLogTable";

export default function App() {
  const [range, setRange] = useState<Range>("all");
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [tab, setTab] = useState<TabId>("logs");
  const [logs, setLogs] = useState<RequestLogRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    setLoading(true);
    try {
      const st = await syncUsage();
      setStatus(st);
      const { since, until } = rangeToWindow(range);
      setSummary(await getSummary(since, until));
      setLogs(await listLogs(since, until, null, 500));
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void refresh(); }, [range]);

  return (
    <div className="min-h-screen bg-gray-50 text-gray-900">
      <Toolbar range={range} onRange={setRange} status={status} onRefresh={refresh} loading={loading} />
      {error && <div className="px-4 pt-3 text-sm text-red-600">{error}</div>}
      <SummaryCards summary={summary} />
      <Tabs active={tab} onChange={setTab} />
      {tab === "logs" && <RequestLogTable rows={logs} />}
    </div>
  );
}
