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

#[tauri::command]
pub fn sync_usage(state: State<'_, Mutex<AppState>>) -> Result<SyncStatus, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;

    let overrides = dao::get_overrides(&conn).map_err(|e| e.to_string())?;

    let ccp = cc_switch_db_path();
    let (pricing, pricing_found) = if ccp.exists() {
        match PricingTable::load(&ccp) {
            Ok(t) if !t.is_empty() => (t, true),
            _ => (PricingTable::from_rows(vec![]), false),
        }
    } else {
        (PricingTable::from_rows(vec![]), false)
    };

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

#[tauri::command]
pub fn set_price_override(state: State<'_, Mutex<AppState>>, model_id: String, input: String, output: String, cache_read: String, cache_creation: String) -> Result<(), String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    let p = crate::pricing::ModelPricing::from_strings(&input, &output, &cache_read, &cache_creation)
        .map_err(|e| e.to_string())?;
    dao::set_override(&conn, &model_id, &p).map_err(|e| e.to_string())
}
