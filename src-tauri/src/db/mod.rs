pub mod dao;
pub mod schema;

use crate::error::AppError;
use rusqlite::Connection;
use std::path::Path;
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
