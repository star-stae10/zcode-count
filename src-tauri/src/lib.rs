pub mod commands;
pub mod db;
pub mod error;
pub mod export;
pub mod pricing;
pub mod zcode;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let db_path = crate::db::default_db_path();
            let own = crate::db::OwnDb::open(&db_path).map_err(|e| e.to_string())?;
            app.manage(std::sync::Mutex::new(crate::commands::AppState { db: own }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::commands::sync_usage,
            crate::commands::get_summary,
            crate::commands::list_logs,
            crate::commands::export_logs,
            crate::commands::get_provider_stats,
            crate::commands::get_model_stats,
            crate::commands::list_provider_models,
            crate::commands::list_provider_names,
            crate::commands::get_unpriced_models,
            crate::commands::preview_clear_usage,
            crate::commands::clear_usage,
            crate::commands::list_audit_logs,
            crate::commands::set_price_override,
            crate::commands::list_price_overrides,
            crate::commands::delete_price_override,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
