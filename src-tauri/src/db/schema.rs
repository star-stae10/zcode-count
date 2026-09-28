use crate::error::AppError;
use rusqlite::{Connection, OptionalExtension};

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
          provider_id TEXT NOT NULL,
          model_id TEXT NOT NULL,
          input_cost_per_million TEXT NOT NULL,
          output_cost_per_million TEXT NOT NULL,
          cache_read_cost_per_million TEXT NOT NULL,
          cache_creation_cost_per_million TEXT NOT NULL,
          -- 峰谷定价（DeepSeek 峰谷覆盖）：NULL = 未启用峰谷
          peak_input_cost_per_million TEXT,
          peak_output_cost_per_million TEXT,
          peak_cache_read_cost_per_million TEXT,
          peak_cache_creation_cost_per_million TEXT,
          PRIMARY KEY (provider_id, model_id)
        );

        -- 清除操作的审计日志（providers/models 存 JSON 数组字符串）。
        CREATE TABLE IF NOT EXISTS audit_log (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          actor TEXT NOT NULL,
          action TEXT NOT NULL,
          started_at_from INTEGER,
          started_at_to INTEGER,
          providers TEXT NOT NULL DEFAULT '[]',
          models TEXT NOT NULL DEFAULT '[]',
          deleted_count INTEGER NOT NULL,
          created_at INTEGER NOT NULL
        );

        -- 清除墓碑：记录已被清除的范围，防止 sync 重叠窗口把已删行从 ZCode 源库
        -- 重新灌回（源库只读，删除只发生在自有库）。provider_id/model_id 为 NULL
        -- 表示不限维度；时间边界为闭区间 [ts_from, ts_to]。
        CREATE TABLE IF NOT EXISTS clear_tombstones (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          ts_from INTEGER NOT NULL,
          ts_to INTEGER NOT NULL,
          provider_id TEXT,
          model_id TEXT,
          created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_tombstones_time ON clear_tombstones(ts_from, ts_to);

        -- 供应商名称快照：sync 时从 ZCode provider 配置（provider_config.json）
        -- upsert 进来，只增不删——provider 从配置中删除后，其历史 ID 仍保留
        -- 同步时捕获的名称。source: zcode_config | manual（manual 为后续扩展）。
        CREATE TABLE IF NOT EXISTS provider_names (
          provider_id   TEXT PRIMARY KEY,
          display_name  TEXT NOT NULL,
          source        TEXT NOT NULL DEFAULT 'zcode_config',
          base_url      TEXT,
          first_seen_at INTEGER NOT NULL,
          updated_at    INTEGER NOT NULL
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

    // 旧 pricing_overrides（仅 model_id）→ 新结构 (provider_id, model_id)。
    // 幂等：仅在表已存在且缺 provider_id 列时重建；旧行的 provider_id 置 ''。
    if table_exists(conn, "pricing_overrides")? && !column_exists(conn, "pricing_overrides", "provider_id")? {
        const LEGACY: &str = "pricing_overrides_legacy_v2";
        // 迁移中断可能留下临时表；先清掉以免 RENAME 冲突。
        if table_exists(conn, LEGACY)? {
            conn.execute(&format!("DROP TABLE {LEGACY}"), [])?;
        }
        // 整段包在 BEGIN IMMEDIATE 事务里；失败回滚，避免半成品结构。
        let result = conn.execute_batch(&format!(
            "BEGIN IMMEDIATE;
             ALTER TABLE pricing_overrides RENAME TO {LEGACY};
             CREATE TABLE pricing_overrides (
               provider_id TEXT NOT NULL,
               model_id TEXT NOT NULL,
               input_cost_per_million TEXT NOT NULL,
               output_cost_per_million TEXT NOT NULL,
               cache_read_cost_per_million TEXT NOT NULL,
               cache_creation_cost_per_million TEXT NOT NULL,
               peak_input_cost_per_million TEXT,
               peak_output_cost_per_million TEXT,
               peak_cache_read_cost_per_million TEXT,
               peak_cache_creation_cost_per_million TEXT,
               PRIMARY KEY (provider_id, model_id)
             );
             INSERT INTO pricing_overrides (provider_id, model_id, input_cost_per_million, output_cost_per_million, cache_read_cost_per_million, cache_creation_cost_per_million)
               SELECT '', model_id, input_cost_per_million, output_cost_per_million, cache_read_cost_per_million, cache_creation_cost_per_million FROM {LEGACY};
             DROP TABLE {LEGACY};
             COMMIT;"
        ));
        if result.is_err() {
            let _ = conn.execute_batch("ROLLBACK;");
        }
        result?;
    }

    // 峰谷定价：pricing_overrides 幂等补 4 个 peak 单价列（NULL = 未启用峰谷）。
    // 存量库升级用；新库与 legacy 重建的 CREATE TABLE 已含这些列。
    for col in [
        "peak_input_cost_per_million",
        "peak_output_cost_per_million",
        "peak_cache_read_cost_per_million",
        "peak_cache_creation_cost_per_million",
    ] {
        if !column_exists(conn, "pricing_overrides", col)? {
            conn.execute(
                &format!("ALTER TABLE pricing_overrides ADD COLUMN {col} TEXT"),
                [],
            )?;
        }
    }

    Ok(())
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, AppError> {
    let found = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |_| Ok(()),
        )
        .optional()?;
    Ok(found.is_some())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, AppError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        if row.get::<_, String>(1)? == column {
            return Ok(true);
        }
    }
    Ok(false)
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

    #[test]
    fn migrates_legacy_pricing_overrides_to_provider_model_key() {
        // 模拟旧库：pricing_overrides 仅有 model_id 主键
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE pricing_overrides (
                model_id TEXT PRIMARY KEY,
                input_cost_per_million TEXT NOT NULL,
                output_cost_per_million TEXT NOT NULL,
                cache_read_cost_per_million TEXT NOT NULL,
                cache_creation_cost_per_million TEXT NOT NULL
            );
            INSERT INTO pricing_overrides VALUES ('legacy-model', '1', '2', '3', '4');",
        ).unwrap();

        // 两次迁移必须幂等
        migrate(&c).unwrap();
        migrate(&c).unwrap();

        // 新结构：provider_id 列存在
        assert!(column_exists(&c, "pricing_overrides", "provider_id").unwrap());
        assert!(column_exists(&c, "pricing_overrides", "model_id").unwrap());
        // 临时表已清理，迁移可重复执行
        assert!(!table_exists(&c, "pricing_overrides_legacy_v2").unwrap(),
                "迁移临时表应被 DROP");

        // 旧行以 provider_id = '' 保留，单价原样
        let (pid, mid, i): (String, String, String) = c
            .query_row(
                "SELECT provider_id, model_id, input_cost_per_million FROM pricing_overrides WHERE model_id='legacy-model'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((pid.as_str(), mid.as_str(), i.as_str()), ("", "legacy-model", "1"));

        // 新表可用：同一 model_id 可挂不同 provider，且 UPSERT 生效
        c.execute(
            "INSERT INTO pricing_overrides (provider_id, model_id, input_cost_per_million, output_cost_per_million, cache_read_cost_per_million, cache_creation_cost_per_million)
             VALUES ('p1','legacy-model','9','9','9','9')
             ON CONFLICT(provider_id, model_id) DO UPDATE SET input_cost_per_million = excluded.input_cost_per_million",
            [],
        ).unwrap();
        c.execute(
            "INSERT INTO pricing_overrides (provider_id, model_id, input_cost_per_million, output_cost_per_million, cache_read_cost_per_million, cache_creation_cost_per_million)
             VALUES ('p1','legacy-model','5','5','5','5')
             ON CONFLICT(provider_id, model_id) DO UPDATE SET input_cost_per_million = excluded.input_cost_per_million",
            [],
        ).unwrap();
        let n: i64 = c
            .query_row("SELECT COUNT(*) FROM pricing_overrides", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2, "应为旧行 + 新 (p1, legacy-model) 共 2 行");
        let new_i: String = c
            .query_row(
                "SELECT input_cost_per_million FROM pricing_overrides WHERE provider_id='p1' AND model_id='legacy-model'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(new_i, "5", "UPSERT 应更新而非插入重复行");
    }

    /// 存量库升级：pricing_overrides 缺 peak 四列时幂等补列（新列允许 NULL）。
    #[test]
    fn migrate_adds_peak_columns_idempotently() {
        // 模拟上一版库：pricing_overrides 无 peak 列
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE pricing_overrides (
                provider_id TEXT NOT NULL,
                model_id TEXT NOT NULL,
                input_cost_per_million TEXT NOT NULL,
                output_cost_per_million TEXT NOT NULL,
                cache_read_cost_per_million TEXT NOT NULL,
                cache_creation_cost_per_million TEXT NOT NULL,
                PRIMARY KEY (provider_id, model_id)
            );",
        ).unwrap();

        // 首次迁移补列；二次迁移必须幂等（重复 ALTER 会因列已存在报错）
        migrate(&c).unwrap();
        migrate(&c).unwrap();

        for col in [
            "peak_input_cost_per_million",
            "peak_output_cost_per_million",
            "peak_cache_read_cost_per_million",
            "peak_cache_creation_cost_per_million",
        ] {
            assert!(column_exists(&c, "pricing_overrides", col).unwrap(), "缺列 {col}");
        }
        // 幂等的直接证据：每个 peak 列恰好出现一次
        let mut stmt = c.prepare("PRAGMA table_info(pricing_overrides)").unwrap();
        let names: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(names.iter().filter(|n| **n == "peak_input_cost_per_million").count(), 1,
                   "peak 列不得重复: {names:?}");
    }
}
