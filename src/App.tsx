import { useEffect, useState } from "react";
import { syncUsage, getSummary, Summary, SyncStatus } from "./lib/api";
import { Range, rangeToWindow } from "./lib/format";
import { Toolbar } from "./components/Toolbar";
import { SummaryCards } from "./components/SummaryCards";

export default function App() {
  const [range, setRange] = useState<Range>("all");
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [loading, setLoading] = useState(false);

  async function refresh() {
    setLoading(true);
    try {
      const st = await syncUsage();
      setStatus(st);
      const { since, until } = rangeToWindow(range);
      setSummary(await getSummary(since, until));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void refresh(); }, [range]);

  return (
    <div className="min-h-screen bg-gray-50 text-gray-900">
      <Toolbar range={range} onRange={setRange} status={status} onRefresh={refresh} loading={loading} />
      <SummaryCards summary={summary} />
    </div>
  );
}
