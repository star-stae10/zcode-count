pub mod dao;
pub mod schema;

use crate::error::AppError;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct OwnDb {
    pub conn: Mutex<Connection>,
}

impl OwnDb {
    pub fn open(path: &Path) -> Result<Self, AppError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        schema::migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }
}

/// 自有库路径：%USERPROFILE%\.zcode-count\zcode-count.db（回退 HOME）。
pub fn default_db_path() -> PathBuf {
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
    PathBuf::from(home).join(".zcode-count").join("zcode-count.db")
}
