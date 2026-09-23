export type TabId = "logs" | "providers" | "models";

const TABS: { id: TabId; label: string }[] = [
  { id: "logs", label: "请求日志" },
  { id: "providers", label: "Provider 统计" },
  { id: "models", label: "模型统计" },
];

export function Tabs({ active, onChange }: { active: TabId; onChange: (t: TabId) => void }) {
  return (
    <div className="flex gap-1 border-b border-gray-200 px-4 pt-3">
      {TABS.map((t) => (
        <button key={t.id} onClick={() => onChange(t.id)}
          className={`rounded-t px-4 py-2 text-sm ${active === t.id ? "border border-b-0 border-gray-200 bg-white font-medium text-blue-600" : "text-gray-500"}`}>
          {t.label}
        </button>
      ))}
    </div>
  );
}
