use crate::db::{dao, OwnDb};
use crate::pricing::{cc_switch_db_path, table::PricingTable};
use crate::zcode::{sync::sync, zcode_db_path};
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;

pub struct AppState {
    pub db: OwnDb,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SyncStatus {
    pub zcode_found: bool,
    pub pricing_found: bool,
    pub imported: i64,
    pub skipped: i64,
    pub unpriced: i64,
    pub last_synced_at: i64,
    pub last_error: Option<String>,
}

/// 读取 cc-switch 定价表（缺失或为空时返回空表，`pricing_found = false`）。
fn load_pricing() -> (PricingTable, bool) {
    let ccp = cc_switch_db_path();
    if ccp.exists() {
        match PricingTable::load(&ccp) {
            Ok(t) if !t.is_empty() => (t, true),
            _ => (PricingTable::from_rows(vec![]), false),
        }
    } else {
        (PricingTable::from_rows(vec![]), false)
    }
}

#[tauri::command]
pub fn sync_usage(state: State<'_, Mutex<AppState>>) -> Result<SyncStatus, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;

    let overrides = dao::get_overrides(&conn).map_err(|e| e.to_string())?;
    let (pricing, pricing_found) = load_pricing();

    let report = sync(&conn, &zcode_db_path(), &pricing, &overrides).map_err(|e| e.to_string())?;
    Ok(SyncStatus {
        zcode_found: report.zcode_found,
        pricing_found,
        imported: report.imported,
        skipped: report.skipped,
        unpriced: report.unpriced,
        last_synced_at: chrono::Utc::now().timestamp_millis(),
        last_error: None,
    })
}

#[tauri::command]
pub fn get_summary(state: State<'_, Mutex<AppState>>, since: i64, until: i64) -> Result<dao::Summary, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::query_summary(&conn, since, until).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_logs(state: State<'_, Mutex<AppState>>, since: i64, until: i64, provider: Option<String>, limit: i64) -> Result<Vec<dao::RequestLogRow>, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::query_logs(&conn, since, until, provider.as_deref(), limit).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_provider_stats(state: State<'_, Mutex<AppState>>, since: i64, until: i64) -> Result<Vec<dao::ProviderStat>, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::query_provider_stats(&conn, since, until).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_model_stats(state: State<'_, Mutex<AppState>>, since: i64, until: i64) -> Result<Vec<dao::ModelStat>, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::query_model_stats(&conn, since, until).map_err(|e| e.to_string())
}

/// 保存「供应商 + 模型」的单价覆盖，立即重算该组合的所有行，返回重算行数。
#[tauri::command]
pub fn set_price_override(
    state: State<'_, Mutex<AppState>>,
    provider_id: String, model_id: String,
    input: String, output: String, cache_read: String, cache_creation: String,
) -> Result<u32, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    let provider_id = provider_id.trim();
    let model_id = model_id.trim();
    if provider_id.is_empty() || model_id.is_empty() {
        return Err("供应商与模型 ID 不能为空".into());
    }
    let p = crate::pricing::ModelPricing::from_strings(&input, &output, &cache_read, &cache_creation)
        .map_err(|e| format!("单价解析失败: {e}"))?;
    crate::pricing::validate_non_negative(&p)?;
    dao::set_override(&conn, provider_id, model_id, &p).map_err(|e| e.to_string())?;

    // 保存后立即重算：重读覆盖与定价表再同步。
    let overrides = dao::get_overrides(&conn).map_err(|e| e.to_string())?;
    let (pricing, _) = load_pricing();
    let report = sync(&conn, &zcode_db_path(), &pricing, &overrides).map_err(|e| e.to_string())?;
    Ok(report.repriced as u32)
}

#[tauri::command]
pub fn list_price_overrides(state: State<'_, Mutex<AppState>>) -> Result<Vec<dao::OverrideRow>, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::list_overrides(&conn).map_err(|e| e.to_string())
}

/// 删除「供应商 + 模型」的覆盖，并让该组合的行重新按当前定价来源回填，返回重算行数。
#[tauri::command]
pub fn delete_price_override(
    state: State<'_, Mutex<AppState>>, provider_id: String, model_id: String,
) -> Result<u32, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    let provider_id = provider_id.trim();
    let model_id = model_id.trim();
    dao::delete_override(&conn, provider_id, model_id).map_err(|e| e.to_string())?;
    // 清掉该组合已按覆盖价定过的行（priced=0），使下方 sync 能按当前定价来源回填。
    dao::clear_pricing_by_provider_model(&conn, provider_id, model_id).map_err(|e| e.to_string())?;

    let overrides = dao::get_overrides(&conn).map_err(|e| e.to_string())?;
    let (pricing, _) = load_pricing();
    let report = sync(&conn, &zcode_db_path(), &pricing, &overrides).map_err(|e| e.to_string())?;
    Ok(report.repriced as u32)
}
