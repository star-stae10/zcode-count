use crate::db::dao::{self, ClearScope, ScopeFilter};
use crate::db::OwnDb;
use crate::pricing::{cc_switch_db_path, table::PricingTable};
use crate::zcode::provider_names::{self, ProviderName};
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
    pub tombstoned: i64,
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

/// 读取 ZCode provider 配置中的名称映射。
/// 配置缺失/损坏一律降级为空列表（名称仅影响展示，绝不阻断 sync）。
fn load_provider_names() -> Vec<ProviderName> {
    let path = provider_names::provider_config_path();
    match provider_names::load(&path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("读取 ZCode provider 配置失败（忽略名称映射）: {e}");
            Vec::new()
        }
    }
}

/// 审计日志的「操作人」：单机工具，取本机 Windows 用户名。
fn current_actor() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "unknown".into())
}

#[tauri::command]
pub fn sync_usage(state: State<'_, Mutex<AppState>>) -> Result<SyncStatus, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;

    let overrides = dao::get_overrides(&conn).map_err(|e| e.to_string())?;
    let (pricing, pricing_found) = load_pricing();
    let names = load_provider_names();

    let report = sync(&conn, &zcode_db_path(), &pricing, &overrides, &names)
        .map_err(|e| e.to_string())?;
    Ok(SyncStatus {
        zcode_found: report.zcode_found,
        pricing_found,
        imported: report.imported,
        skipped: report.skipped,
        tombstoned: report.tombstoned,
        unpriced: report.unpriced,
        last_synced_at: chrono::Utc::now().timestamp_millis(),
        last_error: None,
    })
}

#[tauri::command]
pub fn get_summary(state: State<'_, Mutex<AppState>>, since: i64, until: i64, scope: Option<ScopeFilter>) -> Result<dao::Summary, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::query_summary(&conn, since, until, scope.as_ref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_logs(state: State<'_, Mutex<AppState>>, since: i64, until: i64, scope: Option<ScopeFilter>, limit: i64) -> Result<Vec<dao::RequestLogRow>, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::query_logs(&conn, since, until, scope.as_ref(), limit).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_provider_stats(state: State<'_, Mutex<AppState>>, since: i64, until: i64, scope: Option<ScopeFilter>) -> Result<Vec<dao::ProviderStat>, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::query_provider_stats(&conn, since, until, scope.as_ref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_model_stats(state: State<'_, Mutex<AppState>>, since: i64, until: i64, scope: Option<ScopeFilter>) -> Result<Vec<dao::ModelStat>, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::query_model_stats(&conn, since, until, scope.as_ref()).map_err(|e| e.to_string())
}

/// 库中出现过的全部 (供应商, 模型) 组合（层级选择器的数据源）。
#[tauri::command]
pub fn list_provider_models(state: State<'_, Mutex<AppState>>) -> Result<Vec<dao::ProviderModelRow>, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::list_provider_models(&conn).map_err(|e| e.to_string())
}

/// 供应商名称映射（前端把 provider_id 显示为可读名称；无映射时回退原始 ID）。
#[tauri::command]
pub fn list_provider_names(state: State<'_, Mutex<AppState>>) -> Result<Vec<dao::ProviderNameRow>, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::list_provider_names(&conn).map_err(|e| e.to_string())
}

/// 未定价模型清单（含粗略成本估算区间）。
#[tauri::command]
pub fn get_unpriced_models(state: State<'_, Mutex<AppState>>, since: i64, until: i64, scope: Option<ScopeFilter>) -> Result<Vec<dao::UnpricedModelRow>, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::query_unpriced_models(&conn, since, until, scope.as_ref()).map_err(|e| e.to_string())
}

/// 预览清除范围（不删除）：返回将删除的条数与授权确认短语。
/// 边界问题（时间倒置 / 供应商不存在 / 模型不属于 / 无数据）在此明确报错。
#[tauri::command]
pub fn preview_clear_usage(state: State<'_, Mutex<AppState>>, scope: ClearScope) -> Result<dao::ClearPreview, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::preview_clear(&conn, &scope).map_err(|e| e.to_string())
}

/// 执行清除（危险操作）：后端强制校验授权确认短语；成功后记录审计日志并写清除墓碑。
#[tauri::command]
pub fn clear_usage(state: State<'_, Mutex<AppState>>, scope: ClearScope, confirm: String) -> Result<dao::ClearResult, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let mut conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::clear_records(&mut conn, &scope, &confirm, &current_actor()).map_err(|e| e.to_string())
}

/// 审计日志（清除类操作的历史记录）。
#[tauri::command]
pub fn list_audit_logs(state: State<'_, Mutex<AppState>>, limit: i64) -> Result<Vec<dao::AuditLogRow>, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::list_audit_logs(&conn, limit).map_err(|e| e.to_string())
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
    let names = load_provider_names();
    let report = sync(&conn, &zcode_db_path(), &pricing, &overrides, &names)
        .map_err(|e| e.to_string())?;
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
    // 删除覆盖并按归一化口径清空其影响范围（避免「删了覆盖、成本还留着覆盖价」）。
    dao::delete_override_and_clear(&conn, provider_id, model_id).map_err(|e| e.to_string())?;

    let overrides = dao::get_overrides(&conn).map_err(|e| e.to_string())?;
    let (pricing, _) = load_pricing();
    let names = load_provider_names();
    let report = sync(&conn, &zcode_db_path(), &pricing, &overrides, &names)
        .map_err(|e| e.to_string())?;
    Ok(report.repriced as u32)
}
