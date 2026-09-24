use crate::db::dao::{self, UsageRecord};
use crate::error::AppError;
use crate::pricing::{cost::calculate_cache_inclusive, resolve, table::PricingTable, ModelPricing};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

/// 查询起点相对游标回退的重叠窗口。ZCode 在请求**完成时**才写入 `model_usage`
/// 行，而 `started_at` 是请求**开始**时间；长请求可能在同步之后才入库，其
/// `started_at` 早于水位线。回退该窗口后配合 `INSERT OR IGNORE` 幂等，可避免
/// 「完成晚于同步」的请求被永久漏计。
const SYNC_OVERLAP_MS: i64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Default)]
pub struct SyncReport {
    pub scanned: i64,
    pub imported: i64,
    pub skipped: i64,
    pub unpriced: i64,
    pub repriced: i64,
    pub last_started_at: i64,
    pub zcode_found: bool,
}

struct ZRow {
    id: String,
    provider_id: String,
    model_id: String,
    status: String,
    started_at: i64,
    duration_ms: Option<i64>,
    first_token_ms: Option<i64>,
    input_tokens: i64,
    output_tokens: i64,
    reasoning_tokens: i64,
    cache_read: i64,
    cache_creation: i64,
    session_id: Option<String>,
    query_source: Option<String>,
}

pub fn sync(
    conn: &Connection,
    zcode_path: &Path,
    pricing: &PricingTable,
    overrides: &HashMap<(String, String), ModelPricing>,
) -> Result<SyncReport, AppError> {
    if !zcode_path.exists() {
        return Ok(SyncReport { zcode_found: false, ..Default::default() });
    }
    let source = zcode_path.to_string_lossy().to_string();
    let mtime = max_mtime(zcode_path);
    let cursor = dao::get_cursor(conn, &source)?;
    // 回退重叠窗口：捕获「完成晚于同步」、started_at 已落到水位线之前的请求。
    let since = cursor
        .as_ref()
        .map_or(0, |c| c.last_started_at.saturating_sub(SYNC_OVERLAP_MS));

    // mtime 短路：ZCode 库（含 -wal）自上次同步后未变化 → 不重扫、不写游标。
    // 但仍要执行下方的重定价 pass（定价来源可能在两次同步之间变化）。
    let short_circuit = cursor.as_ref().map_or(false, |c| mtime != 0 && mtime == c.last_mtime);

    let mut report = SyncReport { zcode_found: true, ..Default::default() };

    if short_circuit {
        report.last_started_at = cursor.as_ref().map_or(0, |c| c.last_started_at);
    } else {
        let zconn = Connection::open_with_flags(zcode_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| AppError::Database(format!("打开 ZCode 库失败: {e}")))?;

        let rows = query_rows(&zconn, since)?;
        report.scanned = rows.len() as i64;
        // 从当前水位线起步，避免在「窗口内无新行」时把游标回退。
        let mut max_started = cursor.as_ref().map_or(0, |c| c.last_started_at);

        for r in &rows {
            if r.started_at > max_started {
                max_started = r.started_at;
            }
            let cost = resolve(&r.provider_id, &r.model_id, pricing, overrides)
                .map(|p| calculate_cache_inclusive(r.input_tokens, r.output_tokens, r.cache_read, r.cache_creation, &p));
            let priced = cost.is_some();
            if !priced {
                report.unpriced += 1;
            }
            let (ic, oc, crc, ccc, tc) = match &cost {
                Some(c) => (c.input_cost.to_string(), c.output_cost.to_string(),
                            c.cache_read_cost.to_string(), c.cache_creation_cost.to_string(),
                            c.total_cost.to_string()),
                None => ("0".into(), "0".into(), "0".into(), "0".into(), "0".into()),
            };
            let rec = UsageRecord {
                request_id: r.id.clone(),
                app_type: "zcode".into(),
                provider_id: r.provider_id.clone(),
                model_id: r.model_id.clone(),
                input_tokens: r.input_tokens,
                output_tokens: r.output_tokens,
                reasoning_tokens: r.reasoning_tokens,
                cache_read_tokens: r.cache_read,
                cache_creation_tokens: r.cache_creation,
                input_cost_usd: ic,
                output_cost_usd: oc,
                cache_read_cost_usd: crc,
                cache_creation_cost_usd: ccc,
                total_cost_usd: tc,
                priced,
                started_at: r.started_at,
                duration_ms: r.duration_ms,
                first_token_ms: r.first_token_ms,
                status: r.status.clone(),
                session_id: r.session_id.clone(),
                query_source: r.query_source.clone(),
                created_at: r.started_at,
            };
            if dao::insert_record(conn, &rec)? {
                report.imported += 1;
            } else {
                report.skipped += 1;
            }
        }

        let now = chrono::Utc::now().timestamp_millis();
        dao::set_cursor(conn, &source, max_started, mtime, now)?;
        report.last_started_at = max_started;
    }

    // 重定价 pass（不受 mtime 短路影响：定价来源独立于 ZCode 文件）。
    // (a) 覆盖重算：对每个覆盖的 (provider, model)，重算该组合的**所有**行（含已定价）。
    if !overrides.is_empty() {
        for ((provider_id, model_id), p) in overrides {
            for r in dao::query_records_by_provider_model(conn, provider_id, model_id)? {
                let c = calculate_cache_inclusive(
                    r.input_tokens, r.output_tokens, r.cache_read_tokens, r.cache_creation_tokens, p);
                dao::update_record_pricing(
                    conn, &r.request_id,
                    &c.input_cost.to_string(), &c.output_cost.to_string(),
                    &c.cache_read_cost.to_string(), &c.cache_creation_cost.to_string(),
                    &c.total_cost.to_string(),
                )?;
                report.repriced += 1;
            }
        }
    }

    // (b) 表回填：修复已导入但当时无定价的行（priced=0）。
    if !pricing.is_empty() {
        for r in dao::query_unpriced_records(conn)? {
            if let Some(p) = resolve(&r.provider_id, &r.model_id, pricing, overrides) {
                let c = calculate_cache_inclusive(
                    r.input_tokens, r.output_tokens, r.cache_read_tokens, r.cache_creation_tokens, &p);
                dao::update_record_pricing(
                    conn, &r.request_id,
                    &c.input_cost.to_string(), &c.output_cost.to_string(),
                    &c.cache_read_cost.to_string(), &c.cache_creation_cost.to_string(),
                    &c.total_cost.to_string(),
                )?;
                report.repriced += 1;
            }
        }
    }

    Ok(report)
}

fn max_mtime(path: &Path) -> i64 {
    let mut m = file_mtime(path);
    // 不用 with_extension：非 .sqlite 文件名会被拼错，导致漏同步。
    let wal = std::path::PathBuf::from(format!("{}-wal", path.display()));
    m = m.max(file_mtime(&wal));
    m
}

fn file_mtime(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|md| md.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn query_rows(conn: &Connection, since: i64) -> Result<Vec<ZRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, provider_id, model_id, status, started_at, duration_ms,
                time_to_first_token_ms, input_tokens, output_tokens, reasoning_tokens,
                cache_read_input_tokens, cache_creation_input_tokens, session_id, query_source
         FROM model_usage WHERE started_at >= ?1 ORDER BY started_at ASC",
    )?;
    let it = stmt.query_map([since], |row| {
        Ok(ZRow {
            id: row.get(0)?,
            provider_id: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            model_id: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            status: row.get::<_, Option<String>>(3)?.unwrap_or_else(|| "unknown".into()),
            started_at: row.get(4)?,
            duration_ms: row.get(5)?,
            first_token_ms: row.get(6)?,
            input_tokens: row.get::<_, Option<i64>>(7)?.unwrap_or(0),
            output_tokens: row.get::<_, Option<i64>>(8)?.unwrap_or(0),
            reasoning_tokens: row.get::<_, Option<i64>>(9)?.unwrap_or(0),
            cache_read: row.get::<_, Option<i64>>(10)?.unwrap_or(0),
            cache_creation: row.get::<_, Option<i64>>(11)?.unwrap_or(0),
            session_id: row.get(12)?,
            query_source: row.get(13)?,
        })
    })?;
    let mut out = Vec::new();
    for r in it { out.push(r?); }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::migrate;
    use crate::pricing::{table::PricingTable, ModelPricing};
    use rust_decimal::Decimal;
    use std::collections::HashMap;
    use std::str::FromStr;

    fn setup_zcode(path: &std::path::Path) {
        let c = rusqlite::Connection::open(path).unwrap();
        c.execute_batch(
            "CREATE TABLE model_usage (
                id TEXT PRIMARY KEY, provider_id TEXT, model_id TEXT,
                status TEXT, started_at INTEGER, duration_ms INTEGER,
                time_to_first_token_ms INTEGER, input_tokens INTEGER,
                output_tokens INTEGER, reasoning_tokens INTEGER,
                cache_read_input_tokens INTEGER, cache_creation_input_tokens INTEGER,
                session_id TEXT, query_source TEXT);",
        ).unwrap();
        c.execute(
            "INSERT INTO model_usage VALUES
             ('r1','p1','deepseek-v4-flash','completed',100,1000,200,1000,500,100,800,0,'s1','main_turn'),
             ('r2','p1','deepseek-v4-flash-0731','error',200,900,150,2000,0,0,0,0,'s1','main_turn')",
            [],
        ).unwrap();
    }

    fn table() -> PricingTable {
        PricingTable::from_rows(vec![(
            "deepseek-v4-flash".into(),
            ModelPricing {
                input: Decimal::from_str("0.3").unwrap(),
                output: Decimal::from_str("1.2").unwrap(),
                cache_read: Decimal::from_str("0.006").unwrap(),
                cache_creation: Decimal::ZERO,
            },
        )])
    }

    #[test]
    fn sync_imports_and_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("zc-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zpath = dir.join("db.sqlite");
        let _ = std::fs::remove_file(&zpath);
        setup_zcode(&zpath);

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let overrides = HashMap::new();

        let r1 = sync(&conn, &zpath, &table(), &overrides).unwrap();
        assert_eq!(r1.imported, 2);
        assert_eq!(r1.unpriced, 0);

        let r2 = sync(&conn, &zpath, &table(), &overrides).unwrap();
        assert_eq!(r2.imported, 0); // 幂等

        let logs = crate::db::dao::query_logs(&conn, 0, 1000, None, 100).unwrap();
        assert_eq!(logs.len(), 2);
        // r1: billable input = 1000-800 = 200 -> 200*0.3/1e6 + 500*1.2/1e6 + 800*0.006/1e6
        let r1row = logs.iter().find(|l| l.request_id == "r1").unwrap();
        assert_eq!(r1row.total_cost_usd, "0.0006648");
        assert_eq!(r1row.query_source, Some("main_turn".into()));

        let _ = std::fs::remove_file(&zpath);
    }

    #[test]
    fn missing_zcode_db_is_reported() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let r = sync(&conn, std::path::Path::new("C:/definitely/not/here.sqlite"),
                     &PricingTable::from_rows(vec![]), &HashMap::new()).unwrap();
        assert!(!r.zcode_found);
        assert_eq!(r.imported, 0);
    }

    fn create_usage_table(path: &std::path::Path) {
        let c = rusqlite::Connection::open(path).unwrap();
        c.execute_batch(
            "CREATE TABLE model_usage (
                id TEXT PRIMARY KEY, provider_id TEXT, model_id TEXT,
                status TEXT, started_at INTEGER, duration_ms INTEGER,
                time_to_first_token_ms INTEGER, input_tokens INTEGER,
                output_tokens INTEGER, reasoning_tokens INTEGER,
                cache_read_input_tokens INTEGER, cache_creation_input_tokens INTEGER,
                session_id TEXT, query_source TEXT);",
        ).unwrap();
    }

    fn insert_usage(path: &std::path::Path, id: &str, started_at: i64) {
        let c = rusqlite::Connection::open(path).unwrap();
        c.execute(
            "INSERT INTO model_usage VALUES (?1,'p1','deepseek-v4-flash','completed',?2,0,0,1000,0,0,0,0,'s1','main_turn')",
            rusqlite::params![id, started_at],
        ).unwrap();
    }

    /// 水位线用 `>=`：同一毫秒新出现的行不能被漏掉（若误用 `>` 会漏）。
    #[test]
    fn watermark_does_not_miss_same_millisecond_rows() {
        let dir = std::env::temp_dir().join(format!("zc-test-wm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zpath = dir.join("db.sqlite");
        let _ = std::fs::remove_file(&zpath);
        create_usage_table(&zpath);
        insert_usage(&zpath, "a", 100);

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let overrides = HashMap::new();

        let first = sync(&conn, &zpath, &table(), &overrides).unwrap();
        assert_eq!(first.imported, 1);
        assert_eq!(first.last_started_at, 100);

        let src = zpath.to_string_lossy().to_string();
        let cursor_mtime = crate::db::dao::get_cursor(&conn, &src).unwrap().unwrap().last_mtime;

        // 确保 mtime 变化，从而不会走短路（否则测不到水位线）
        std::thread::sleep(std::time::Duration::from_millis(10));
        // 同毫秒（100）新增 b，并新增更晚的 c
        insert_usage(&zpath, "b", 100);
        insert_usage(&zpath, "c", 200);
        assert_ne!(max_mtime(&zpath), cursor_mtime, "mtime 未变化，测试前提不成立");

        let second = sync(&conn, &zpath, &table(), &overrides).unwrap();
        assert_eq!(second.imported, 2); // b（同毫秒）与 c 都必须导入
        let logs = crate::db::dao::query_logs(&conn, 0, 1000, None, 100).unwrap();
        assert_eq!(logs.len(), 3);

        let _ = std::fs::remove_file(&zpath);
    }

    /// 「完成晚于同步」的请求：ZCode 在请求完成时才写 `model_usage` 行，
    /// 其 `started_at` 可能早于水位线。查询起点必须回退 `SYNC_OVERLAP_MS`，
    /// 否则该行永久漏计。
    #[test]
    fn late_completed_request_before_watermark_is_imported() {
        const DAY: i64 = 24 * 60 * 60 * 1000;
        let dir = std::env::temp_dir().join(format!("zc-test-late-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zpath = dir.join("db.sqlite");
        let _ = std::fs::remove_file(&zpath);
        create_usage_table(&zpath);
        let first_started = 100 * DAY;
        insert_usage(&zpath, "first", first_started);

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let overrides = HashMap::new();

        let first = sync(&conn, &zpath, &table(), &overrides).unwrap();
        assert_eq!(first.imported, 1);
        assert_eq!(first.last_started_at, first_started);

        let src = zpath.to_string_lossy().to_string();
        let cursor_mtime = crate::db::dao::get_cursor(&conn, &src).unwrap().unwrap().last_mtime;

        // 模拟「完成晚于同步」：插入一行 started_at 早于水位线、但此刻才出现的请求。
        std::thread::sleep(std::time::Duration::from_millis(10));
        insert_usage(&zpath, "late", first_started - 3 * DAY);
        assert_ne!(max_mtime(&zpath), cursor_mtime, "mtime 未变化，测试前提不成立");

        let second = sync(&conn, &zpath, &table(), &overrides).unwrap();
        assert!(second.imported >= 1, "早于水位线的迟到行必须被导入，imported={}", second.imported);
        let logs = crate::db::dao::query_logs(&conn, 0, i64::MAX, None, 100).unwrap();
        assert!(logs.iter().any(|l| l.request_id == "late"), "query_logs 应能查到迟到行");

        let _ = std::fs::remove_file(&zpath);
    }

    /// 已导入的 `priced=0` 行，在定价稍后可用时必须被重定价
    /// （即使 ZCode 库自上次同步后未变化、走 mtime 短路）。
    #[test]
    fn unpriced_rows_are_repriced_when_pricing_appears() {
        let dir = std::env::temp_dir().join(format!("zc-test-reprice-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zpath = dir.join("db.sqlite");
        let _ = std::fs::remove_file(&zpath);
        create_usage_table(&zpath);
        insert_usage(&zpath, "r", 1000);

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let overrides = HashMap::new();

        // 第一次：无定价表 → 行 priced=0
        let empty = PricingTable::from_rows(vec![]);
        let first = sync(&conn, &zpath, &empty, &overrides).unwrap();
        assert_eq!(first.imported, 1);
        assert_eq!(first.unpriced, 1);

        let before = crate::db::dao::query_logs(&conn, 0, i64::MAX, None, 100).unwrap();
        let b = before.iter().find(|l| l.request_id == "r").unwrap();
        assert!(!b.priced);
        assert_eq!(b.total_cost_usd, "0");

        // 第二次：定价可用；ZCode 库未变化（mtime 短路），重定价仍须执行。
        let second = sync(&conn, &zpath, &table(), &overrides).unwrap();
        assert_eq!(second.repriced, 1, "应重定价 1 行");

        let after = crate::db::dao::query_logs(&conn, 0, i64::MAX, None, 100).unwrap();
        let r = after.iter().find(|l| l.request_id == "r").unwrap();
        assert!(r.priced);
        // input=1000, output=0, cache_read=0 → 1000*0.3/1e6 = 0.0003
        assert_eq!(r.total_cost_usd, "0.0003");

        let _ = std::fs::remove_file(&zpath);
    }

    /// 覆盖优先：给 `(p1, m1)` 设覆盖后，该组合**所有**行（含原已按表定价的）被重算为覆盖价，
    /// 其它组合不变；且覆盖单独存在（不依赖 pricing 表）时也触发重算。
    #[test]
    fn override_reprices_all_rows_of_combo_and_leaves_others() {
        let dir = std::env::temp_dir().join(format!("zc-test-ovr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zpath = dir.join("db.sqlite");
        let _ = std::fs::remove_file(&zpath);
        create_usage_table(&zpath);
        {
            let c = rusqlite::Connection::open(&zpath).unwrap();
            c.execute_batch(
                "INSERT INTO model_usage VALUES
                 ('r1','p1','m1','completed',100,0,0,1000,0,0,0,0,'s1','main_turn'),
                 ('r2','p1','m1','completed',200,0,0,2000,0,0,0,0,'s1','main_turn'),
                 ('r3','p1','m2','completed',300,0,0,1000,0,0,0,0,'s1','main_turn');",
            ).unwrap();
        }

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let table = PricingTable::from_rows(vec![
            ("m1".into(), ModelPricing { input: Decimal::from_str("0.3").unwrap(), ..pricing_zero() }),
            ("m2".into(), ModelPricing { input: Decimal::from_str("0.3").unwrap(), ..pricing_zero() }),
        ]);

        // 第一次：无覆盖 → 全部按表定价
        let first = sync(&conn, &zpath, &table, &HashMap::new()).unwrap();
        assert_eq!(first.imported, 3);
        let logs = crate::db::dao::query_logs(&conn, 0, i64::MAX, None, 100).unwrap();
        let cost = |id: &str| Decimal::from_str(
            &logs.iter().find(|l| l.request_id == id).unwrap().total_cost_usd).unwrap();
        assert_eq!(cost("r1"), Decimal::from_str("0.0003").unwrap());
        assert_eq!(cost("r2"), Decimal::from_str("0.0006").unwrap());
        assert_eq!(cost("r3"), Decimal::from_str("0.0003").unwrap());

        // 第二次：设覆盖 (p1, m1) 输入价 1.0；ZCode 文件未变（mtime 短路），覆盖重算仍须执行。
        let mut overrides = HashMap::new();
        overrides.insert(
            ("p1".to_string(), "m1".to_string()),
            ModelPricing { input: Decimal::from_str("1.0").unwrap(), ..pricing_zero() },
        );
        let second = sync(&conn, &zpath, &table, &overrides).unwrap();
        assert_eq!(second.repriced, 2, "该组合 2 行（含已定价行）都应重算");

        let logs = crate::db::dao::query_logs(&conn, 0, i64::MAX, None, 100).unwrap();
        let cost = |id: &str| Decimal::from_str(
            &logs.iter().find(|l| l.request_id == id).unwrap().total_cost_usd).unwrap();
        assert_eq!(cost("r1"), Decimal::from_str("0.001").unwrap(), "r1 用覆盖价 1.0");
        assert_eq!(cost("r2"), Decimal::from_str("0.002").unwrap(), "r2 用覆盖价 1.0");
        assert_eq!(cost("r3"), Decimal::from_str("0.0003").unwrap(), "其它组合不受影响");

        // 覆盖单独存在（pricing 表为空）也应触发重算。
        let mut overrides2 = HashMap::new();
        overrides2.insert(
            ("p1".to_string(), "m1".to_string()),
            ModelPricing { input: Decimal::from_str("2.0").unwrap(), ..pricing_zero() },
        );
        let third = sync(&conn, &zpath, &PricingTable::from_rows(vec![]), &overrides2).unwrap();
        assert_eq!(third.repriced, 2, "空定价表 + 有覆盖也必须重算");
        let logs = crate::db::dao::query_logs(&conn, 0, i64::MAX, None, 100).unwrap();
        let cost = |id: &str| Decimal::from_str(
            &logs.iter().find(|l| l.request_id == id).unwrap().total_cost_usd).unwrap();
        assert_eq!(cost("r1"), Decimal::from_str("0.002").unwrap());

        let _ = std::fs::remove_file(&zpath);
    }

    /// 删除覆盖后：清空该组合成本并重新回填——表里有价 → 回退表价；表里无价 → 变未定价。
    /// 其它组合的行不受影响。
    #[test]
    fn deleting_override_reverts_to_table_or_unpriced() {
        let dir = std::env::temp_dir().join(format!("zc-test-del-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zpath = dir.join("db.sqlite");
        let _ = std::fs::remove_file(&zpath);
        create_usage_table(&zpath);
        {
            let c = rusqlite::Connection::open(&zpath).unwrap();
            c.execute_batch(
                "INSERT INTO model_usage VALUES
                 ('r1','p1','m1','completed',100,0,0,1000,0,0,0,0,'s1','main_turn'),
                 ('r2','p1','m2','completed',200,0,0,1000,0,0,0,0,'s1','main_turn'),
                 ('r3','p1','m3','completed',300,0,0,1000,0,0,0,0,'s1','main_turn');",
            ).unwrap();
        }

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        // 表里只有 m1 的价；m2/m3 查不到
        let table = PricingTable::from_rows(vec![
            ("m1".into(), ModelPricing { input: Decimal::from_str("0.3").unwrap(), ..pricing_zero() }),
        ]);
        let mut overrides = HashMap::new();
        for m in ["m1", "m2", "m3"] {
            let p = ModelPricing { input: Decimal::from_str("1.0").unwrap(), ..pricing_zero() };
            dao::set_override(&conn, "p1", m, &p).unwrap();
            overrides.insert(("p1".to_string(), m.to_string()), p);
        }
        let first = sync(&conn, &zpath, &table, &overrides).unwrap();
        assert_eq!(first.imported, 3);
        let cost = |id: &str| {
            let logs = crate::db::dao::query_logs(&conn, 0, i64::MAX, None, 100).unwrap();
            let r = logs.into_iter().find(|l| l.request_id == id).unwrap();
            (r.priced, Decimal::from_str(&r.total_cost_usd).unwrap())
        };
        for id in ["r1", "r2", "r3"] {
            let (priced, c) = cost(id);
            assert!(priced);
            assert_eq!(c, Decimal::from_str("0.001").unwrap(), "{id} 应为覆盖价");
        }

        // 删除 (p1,m1) 与 (p1,m2) 的覆盖，并清空其成本后重新同步；保留 (p1,m3)。
        for m in ["m1", "m2"] {
            dao::delete_override(&conn, "p1", m).unwrap();
            let n = dao::clear_pricing_by_provider_model(&conn, "p1", m).unwrap();
            assert_eq!(n, 1, "组合 (p1,{m}) 应清空 1 行");
        }
        let remaining = dao::get_overrides(&conn).unwrap();
        assert_eq!(remaining.len(), 1, "只剩 (p1,m3) 覆盖");
        let second = sync(&conn, &zpath, &table, &remaining).unwrap();
        assert_eq!(second.repriced, 2, "r1 回填表价 + r2 保持未定价不计数；仅 r1 计入");

        // 表里有价 → 回退表价
        let (priced, c) = cost("r1");
        assert!(priced, "r1 应回退为表价（priced=1）");
        assert_eq!(c, Decimal::from_str("0.0003").unwrap());
        // 表里无价 → 未定价、成本 0
        let (priced, c) = cost("r2");
        assert!(!priced, "r2 应变为未定价");
        assert_eq!(c, Decimal::ZERO);
        // 其它组合不受影响
        let (priced, c) = cost("r3");
        assert!(priced);
        assert_eq!(c, Decimal::from_str("0.001").unwrap(), "r3 的覆盖仍在，成本不变");

        let _ = std::fs::remove_file(&zpath);
    }

    fn pricing_zero() -> ModelPricing {
        ModelPricing {
            input: Decimal::ZERO, output: Decimal::ZERO,
            cache_read: Decimal::ZERO, cache_creation: Decimal::ZERO,
        }
    }

    /// 文件未变化（含 -wal）→ 第二次 sync 短路：不重扫、不写游标。
    #[test]
    fn unchanged_file_short_circuits() {
        let dir = std::env::temp_dir().join(format!("zc-test-sc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zpath = dir.join("db.sqlite");
        let _ = std::fs::remove_file(&zpath);
        setup_zcode(&zpath);

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let overrides = HashMap::new();

        let first = sync(&conn, &zpath, &table(), &overrides).unwrap();
        assert_eq!(first.scanned, 2);
        assert_eq!(first.imported, 2);

        let second = sync(&conn, &zpath, &table(), &overrides).unwrap();
        assert_eq!(second.scanned, 0); // 短路
        assert_eq!(second.imported, 0);
        assert_eq!(second.last_started_at, 200);

        let logs = crate::db::dao::query_logs(&conn, 0, 1000, None, 100).unwrap();
        assert_eq!(logs.len(), 2); // 记录数不变

        let _ = std::fs::remove_file(&zpath);
    }

    /// 强制走插入路径：改写 ZCode 库使 mtime 变化，重复 request_id 必须被 INSERT OR IGNORE 丢弃。
    #[test]
    fn duplicate_request_id_not_reimported() {
        let dir = std::env::temp_dir().join(format!("zc-test-dup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zpath = dir.join("db.sqlite");
        let _ = std::fs::remove_file(&zpath);
        create_usage_table(&zpath);
        insert_usage(&zpath, "a", 100);

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let overrides = HashMap::new();

        let first = sync(&conn, &zpath, &table(), &overrides).unwrap();
        assert_eq!(first.imported, 1);

        let src = zpath.to_string_lossy().to_string();
        let cursor_mtime = crate::db::dao::get_cursor(&conn, &src).unwrap().unwrap().last_mtime;

        // 改写同一行（request_id 仍为 'a'），并确保 mtime 变化以绕过短路
        std::thread::sleep(std::time::Duration::from_millis(10));
        let c = rusqlite::Connection::open(&zpath).unwrap();
        c.execute("UPDATE model_usage SET status='error' WHERE id='a'", []).unwrap();
        drop(c);
        assert_ne!(max_mtime(&zpath), cursor_mtime, "mtime 未变化，测试前提不成立");

        let second = sync(&conn, &zpath, &table(), &overrides).unwrap();
        assert_eq!(second.scanned, 1); // 确实走了查询/插入路径
        assert_eq!(second.imported, 0); // 重复 request_id 未再导入
        assert_eq!(second.skipped, 1);

        let logs = crate::db::dao::query_logs(&conn, 0, 1000, None, 100).unwrap();
        assert_eq!(logs.len(), 1); // 记录数不变

        let _ = std::fs::remove_file(&zpath);
    }

    /// 游标 mtime 取 db.sqlite 与 db.sqlite-wal 的最大值（WAL 变更也算变更）。
    #[test]
    fn max_mtime_includes_wal() {
        let dir = std::env::temp_dir().join(format!("zc-test-wal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zpath = dir.join("db.sqlite");
        let wal = dir.join("db.sqlite-wal");
        let _ = std::fs::remove_file(&zpath);
        let _ = std::fs::remove_file(&wal);

        std::fs::write(&zpath, b"x").unwrap();
        let db_mtime = file_mtime(&zpath);
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&wal, b"y").unwrap();
        let wal_mtime = file_mtime(&wal);

        assert!(wal_mtime > db_mtime, "wal_mtime={wal_mtime} db_mtime={db_mtime}");
        assert_eq!(max_mtime(&zpath), wal_mtime);

        let _ = std::fs::remove_file(&zpath);
        let _ = std::fs::remove_file(&wal);
    }
}
