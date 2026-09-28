/**
 * 供应商显示名称映射：provider_id → 可读名称（来自 provider_names 表，App 启动时拉取）。
 * 铁律：显示才用名称，所有筛选/传参/写入仍用原始 provider_id。
 */
export type ProviderNames = Record<string, string>;

/** 有映射返回名称，无映射回退原始 ID（不报错）。 */
export function providerLabel(names: ProviderNames, id: string): string {
  return names[id] ?? id;
}

/** provider_id 列表 → 显示文本（「、」分隔）；空列表返回 empty 文案。 */
export function providerListLabel(names: ProviderNames, ids: string[], empty: string): string {
  return ids.length > 0 ? ids.map((p) => providerLabel(names, p)).join("、") : empty;
}

/** 列表中是否存在可映射的名称（用于决定是否输出 title 保留原始 ID）。 */
export function hasAnyName(names: ProviderNames, ids: string[]): boolean {
  return ids.some((p) => names[p] != null);
}
