import { invoke } from "@tauri-apps/api/core";

export interface Summary {
  input_tokens: number; output_tokens: number; reasoning_tokens: number;
  cache_read_tokens: number; cache_creation_tokens: number;
  request_count: number; session_count: number;
  total_cost_usd: string; unpriced_count: number;
  unpriced_tokens: number; unpriced_models: number;
}
export interface RequestLogRow {
  request_id: string; provider_id: string; model_id: string;
  input_tokens: number; output_tokens: number; cache_read_tokens: number;
  total_cost_usd: string; priced: boolean;
  duration_ms: number | null; first_token_ms: number | null;
  status: string; started_at: number;
  query_source: string | null;
}
export interface ProviderStat {
  provider_id: string; request_count: number;
  input_tokens: number; output_tokens: number; total_cost_usd: string;
  unpriced_count: number;
}
export interface ModelStat {
  model_id: string; request_count: number;
  input_tokens: number; output_tokens: number; total_cost_usd: string;
  unpriced_count: number;
}
export interface SyncStatus {
  zcode_found: boolean; pricing_found: boolean;
  imported: number; skipped: number; tombstoned: number; unpriced: number;
  last_synced_at: number; last_error: string | null;
}
export interface PriceOverride {
  provider_id: string; model_id: string;
  input: string; output: string; cache_read: string; cache_creation: string;
  /** 该覆盖当前命中的用量记录条数（0 = 未生效，需检查模型 ID 是否与记录一致）。 */
  matched_count: number;
}

/** 供应商名称映射行（来自自有库 provider_names 表，sync 时从 ZCode 配置快照）。 */
export interface ProviderNameRow {
  provider_id: string; display_name: string; source: string;
  base_url: string | null;
}

/** 「供应商 + 模型」组合（模型级范围选择）。 */
export interface ModelSel {
  provider_id: string; model_id: string;
}

/**
 * 核算范围筛选（供应商级 + 模型级并集生效；两者都空 = 不筛选，即全部）。
 * 仅影响统计与展示，不会删除任何数据。
 */
export interface ScopeFilter {
  providers: string[];
  models: ModelSel[];
}

/** 供应商 > 模型层级选择器数据源的一行。 */
export interface ProviderModelRow {
  provider_id: string; model_id: string;
}

/** 清除范围（时间段 + 供应商/模型并集；全空 = 清除全部）。 */
export interface ClearScope {
  since: number | null; until: number | null;
  providers: string[];
  models: ModelSel[];
}

/** 清除预览：将删除的条数与授权确认短语。 */
export interface ClearPreview {
  deleted_count: number; confirm_token: string;
  since: number | null; until: number | null;
  providers: string[]; models: ModelSel[];
}

/** 清除结果反馈。 */
export interface ClearResult {
  deleted_count: number;
  affected_since: number | null; affected_until: number | null;
  affected_providers: string[]; affected_models: ModelSel[];
  audit_id: number;
  cursor_reset: boolean;
}

/** 清除操作的审计日志行。 */
export interface AuditLogRow {
  id: number; actor: string; action: string;
  started_at_from: number | null; started_at_to: number | null;
  providers: string[]; models: ModelSel[];
  deleted_count: number; created_at: number;
}

/** 未定价模型清单行（估算区间无可参照时为 null）。 */
export interface UnpricedModelRow {
  provider_id: string; model_id: string;
  request_count: number; total_tokens: number;
  first_started_at: number; last_started_at: number;
  est_cost_low_usd: string | null; est_cost_high_usd: string | null;
}

export const syncUsage = () => invoke<SyncStatus>("sync_usage");
export const getSummary = (since: number, until: number, scope: ScopeFilter | null) =>
  invoke<Summary>("get_summary", { since, until, scope });
export const listLogs = (since: number, until: number, scope: ScopeFilter | null, limit: number) =>
  invoke<RequestLogRow[]>("list_logs", { since, until, scope, limit });
export const getProviderStats = (since: number, until: number, scope: ScopeFilter | null) =>
  invoke<ProviderStat[]>("get_provider_stats", { since, until, scope });
export const getModelStats = (since: number, until: number, scope: ScopeFilter | null) =>
  invoke<ModelStat[]>("get_model_stats", { since, until, scope });
export const listProviderModels = () =>
  invoke<ProviderModelRow[]>("list_provider_models");
export const listProviderNames = () =>
  invoke<ProviderNameRow[]>("list_provider_names");
export const getUnpricedModels = (since: number, until: number, scope: ScopeFilter | null) =>
  invoke<UnpricedModelRow[]>("get_unpriced_models", { since, until, scope });

// 清除：预览（不删除）与执行（永久删除，需授权确认短语）。
export const previewClearUsage = (scope: ClearScope) =>
  invoke<ClearPreview>("preview_clear_usage", { scope });
export const clearUsage = (scope: ClearScope, confirm: string) =>
  invoke<ClearResult>("clear_usage", { scope, confirm });
export const listAuditLogs = (limit: number) =>
  invoke<AuditLogRow[]>("list_audit_logs", { limit });

// 注意：Tauri v2 参数默认 camelCase 映射到 Rust 的 snake_case。
export const setPriceOverride = (providerId: string, modelId: string, input: string, output: string, cacheRead: string, cacheCreation: string) =>
  invoke<number>("set_price_override", { providerId, modelId, input, output, cacheRead, cacheCreation });
export const listPriceOverrides = () => invoke<PriceOverride[]>("list_price_overrides");
export const deletePriceOverride = (providerId: string, modelId: string) =>
  invoke<number>("delete_price_override", { providerId, modelId });
