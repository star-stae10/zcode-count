pub mod commands;
pub mod db;
pub mod error;
pub mod pricing;
pub mod zcode;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
            crate::commands::get_provider_stats,
            crate::commands::get_model_stats,
            crate::commands::set_price_override,
            crate::commands::list_price_overrides,
            crate::commands::delete_price_override,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
