import { invoke } from "@tauri-apps/api/core";

export interface Summary {
  input_tokens: number; output_tokens: number; reasoning_tokens: number;
  cache_read_tokens: number; cache_creation_tokens: number;
  request_count: number; session_count: number;
  total_cost_usd: string; unpriced_count: number;
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
  imported: number; skipped: number; unpriced: number;
  last_synced_at: number; last_error: string | null;
}

export const syncUsage = () => invoke<SyncStatus>("sync_usage");
export const getSummary = (since: number, until: number) => invoke<Summary>("get_summary", { since, until });
export const listLogs = (since: number, until: number, provider: string | null, limit: number) =>
  invoke<RequestLogRow[]>("list_logs", { since, until, provider, limit });
export const getProviderStats = (since: number, until: number) => invoke<ProviderStat[]>("get_provider_stats", { since, until });
export const getModelStats = (since: number, until: number) => invoke<ModelStat[]>("get_model_stats", { since, until });
