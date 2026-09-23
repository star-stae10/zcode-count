pub mod sync;

use std::path::PathBuf;

/// ZCode 用量库路径：%USERPROFILE%\.zcode\cli\db\db.sqlite
pub fn zcode_db_path() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    PathBuf::from(home).join(".zcode").join("cli").join("db").join("db.sqlite")
}
