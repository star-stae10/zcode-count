use crate::error::AppError;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS usage_records (
          request_id TEXT PRIMARY KEY,
          app_type TEXT NOT NULL DEFAULT 'zcode',
          provider_id TEXT NOT NULL,
          model_id TEXT NOT NULL,
          query_source TEXT,
          input_tokens INTEGER NOT NULL DEFAULT 0,
          output_tokens INTEGER NOT NULL DEFAULT 0,
          reasoning_tokens INTEGER NOT NULL DEFAULT 0,
          cache_read_tokens INTEGER NOT NULL DEFAULT 0,
          cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
          input_cost_usd TEXT NOT NULL DEFAULT '0',
          output_cost_usd TEXT NOT NULL DEFAULT '0',
          cache_read_cost_usd TEXT NOT NULL DEFAULT '0',
          cache_creation_cost_usd TEXT NOT NULL DEFAULT '0',
          total_cost_usd TEXT NOT NULL DEFAULT '0',
          priced INTEGER NOT NULL DEFAULT 0,
          started_at INTEGER NOT NULL,
          duration_ms INTEGER,
          first_token_ms INTEGER,
          status TEXT NOT NULL,
          session_id TEXT,
          created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_usage_started ON usage_records(started_at);
        CREATE INDEX IF NOT EXISTS idx_usage_model ON usage_records(model_id);
        CREATE INDEX IF NOT EXISTS idx_usage_provider ON usage_records(provider_id);

        CREATE TABLE IF NOT EXISTS sync_cursors (
          source TEXT PRIMARY KEY,
          last_started_at INTEGER NOT NULL DEFAULT 0,
          last_mtime INTEGER NOT NULL DEFAULT 0,
          last_synced_at INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS pricing_overrides (
          model_id TEXT PRIMARY KEY,
          input_cost_per_million TEXT NOT NULL,
          output_cost_per_million TEXT NOT NULL,
          cache_read_cost_per_million TEXT NOT NULL,
          cache_creation_cost_per_million TEXT NOT NULL
        );
        "#,
    )?;

    // 兼容早期开发库：缺列则补（新库 CREATE TABLE 已含该列）
    let has_query_source = {
        let mut stmt = conn.prepare("PRAGMA table_info(usage_records)")?;
        let mut rows = stmt.query([])?;
        let mut found = false;
        while let Some(row) = rows.next()? {
            if row.get::<_, String>(1)? == "query_source" {
                found = true;
                break;
            }
        }
        found
    };
    if !has_query_source {
        conn.execute("ALTER TABLE usage_records ADD COLUMN query_source TEXT", [])?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_is_idempotent_and_backfills_query_source() {
        // 模拟早期开发库：usage_records 已存在但没有 query_source 列
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE usage_records (
                request_id TEXT PRIMARY KEY,
                app_type TEXT NOT NULL DEFAULT 'zcode',
                provider_id TEXT NOT NULL,
                model_id TEXT NOT NULL,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                reasoning_tokens INTEGER NOT NULL DEFAULT 0,
                cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
                input_cost_usd TEXT NOT NULL DEFAULT '0',
                output_cost_usd TEXT NOT NULL DEFAULT '0',
                cache_read_cost_usd TEXT NOT NULL DEFAULT '0',
                cache_creation_cost_usd TEXT NOT NULL DEFAULT '0',
                total_cost_usd TEXT NOT NULL DEFAULT '0',
                priced INTEGER NOT NULL DEFAULT 0,
                started_at INTEGER NOT NULL,
                duration_ms INTEGER,
                first_token_ms INTEGER,
                status TEXT NOT NULL,
                session_id TEXT,
                created_at INTEGER NOT NULL
            );",
        ).unwrap();

        // 首次迁移补列；二次迁移必须幂等（不报错、不重复加列）
        migrate(&c).unwrap();
        migrate(&c).unwrap();

        let mut stmt = c.prepare("PRAGMA table_info(usage_records)").unwrap();
        let names: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(names.iter().filter(|n| *n == "query_source").count(), 1,
                   "query_source 列应恰好存在一次: {names:?}");
    }
}
