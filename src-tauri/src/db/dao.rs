use crate::error::AppError;
use crate::pricing::ModelPricing;
use rusqlite::{params, Connection, OptionalExtension};
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize)]
pub struct UsageRecord {
    pub request_id: String,
    pub app_type: String,
    pub provider_id: String,
    pub model_id: String,
    pub query_source: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub input_cost_usd: String,
    pub output_cost_usd: String,
    pub cache_read_cost_usd: String,
    pub cache_creation_cost_usd: String,
    pub total_cost_usd: String,
    pub priced: bool,
    pub started_at: i64,
    pub duration_ms: Option<i64>,
    pub first_token_ms: Option<i64>,
    pub status: String,
    pub session_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Cursor {
    pub last_started_at: i64,
    pub last_mtime: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Summary {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub request_count: i64,
    pub session_count: i64,
    pub total_cost_usd: String,
    pub unpriced_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestLogRow {
    pub request_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub total_cost_usd: String,
    pub priced: bool,
    pub duration_ms: Option<i64>,
    pub first_token_ms: Option<i64>,
    pub status: String,
    pub started_at: i64,
    pub query_source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStat {
    pub provider_id: String,
    pub request_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_cost_usd: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelStat {
    pub model_id: String,
    pub request_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_cost_usd: String,
}

pub fn insert_record(conn: &Connection, r: &UsageRecord) -> Result<bool, AppError> {
    let n = conn.execute(
        "INSERT OR IGNORE INTO usage_records (
            request_id, app_type, provider_id, model_id, query_source,
            input_tokens, output_tokens, reasoning_tokens,
            cache_read_tokens, cache_creation_tokens,
            input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd,
            total_cost_usd, priced, started_at, duration_ms, first_token_ms,
            status, session_id, created_at
        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)",
        params![
            r.request_id, r.app_type, r.provider_id, r.model_id, r.query_source,
            r.input_tokens, r.output_tokens, r.reasoning_tokens,
            r.cache_read_tokens, r.cache_creation_tokens,
            r.input_cost_usd, r.output_cost_usd, r.cache_read_cost_usd, r.cache_creation_cost_usd,
            r.total_cost_usd, r.priced as i64, r.started_at, r.duration_ms, r.first_token_ms,
            r.status, r.session_id, r.created_at,
        ],
    )?;
    Ok(n > 0)
}

pub fn get_cursor(conn: &Connection, source: &str) -> Result<Option<Cursor>, AppError> {
    Ok(conn
        .query_row(
            "SELECT last_started_at, last_mtime FROM sync_cursors WHERE source = ?1",
            [source],
            |row| Ok(Cursor { last_started_at: row.get(0)?, last_mtime: row.get(1)? }),
        )
        .optional()?)
}

pub fn set_cursor(conn: &Connection, source: &str, last_started_at: i64, last_mtime: i64, synced_at: i64) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO sync_cursors (source, last_started_at, last_mtime, last_synced_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(source) DO UPDATE SET
           last_started_at = excluded.last_started_at,
           last_mtime = excluded.last_mtime,
           last_synced_at = excluded.last_synced_at",
        params![source, last_started_at, last_mtime, synced_at],
    )?;
    Ok(())
}

pub fn query_summary(conn: &Connection, since: i64, until: i64) -> Result<Summary, AppError> {
    let mut stmt = conn.prepare(
        "SELECT input_tokens, output_tokens, reasoning_tokens, cache_read_tokens,
                cache_creation_tokens, session_id, total_cost_usd, priced
         FROM usage_records WHERE started_at >= ?1 AND started_at <= ?2",
    )?;
    let it = stmt.query_map(params![since, until], |row| {
        Ok((
            row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?, row.get::<_, i64>(4)?,
            row.get::<_, Option<String>>(5)?, row.get::<_, String>(6)?,
            row.get::<_, i64>(7)?,
        ))
    })?;

    let mut s = Summary::default();
    let mut total = Decimal::ZERO;
    let mut sessions = std::collections::HashSet::new();
    for r in it {
        let (i, o, re, cr, cc, sess, cost, priced) = r?;
        s.input_tokens += i;
        s.output_tokens += o;
        s.reasoning_tokens += re;
        s.cache_read_tokens += cr;
        s.cache_creation_tokens += cc;
        s.request_count += 1;
        if let Some(x) = sess { sessions.insert(x); }
        if priced == 0 { s.unpriced_count += 1; }
        total += Decimal::from_str(&cost).unwrap_or(Decimal::ZERO);
    }
    s.session_count = sessions.len() as i64;
    s.total_cost_usd = total.normalize().to_string();
    Ok(s)
}

pub fn query_logs(conn: &Connection, since: i64, until: i64, provider: Option<&str>, limit: i64) -> Result<Vec<RequestLogRow>, AppError> {
    let mut sql = String::from(
        "SELECT request_id, provider_id, model_id, input_tokens, output_tokens,
                cache_read_tokens, total_cost_usd, priced, duration_ms, first_token_ms,
                status, started_at, query_source
         FROM usage_records WHERE started_at >= ?1 AND started_at <= ?2",
    );
    if provider.is_some() {
        sql.push_str(" AND provider_id = ?3");
    }
    sql.push_str(" ORDER BY started_at DESC LIMIT ?");
    let limit_idx = if provider.is_some() { 4 } else { 3 };
    sql.push_str(&limit_idx.to_string());

    let map_row = |row: &rusqlite::Row| -> rusqlite::Result<RequestLogRow> {
        Ok(RequestLogRow {
            request_id: row.get(0)?,
            provider_id: row.get(1)?,
            model_id: row.get(2)?,
            input_tokens: row.get(3)?,
            output_tokens: row.get(4)?,
            cache_read_tokens: row.get(5)?,
            total_cost_usd: row.get(6)?,
            priced: row.get::<_, i64>(7)? != 0,
            duration_ms: row.get(8)?,
            first_token_ms: row.get(9)?,
            status: row.get(10)?,
            started_at: row.get(11)?,
            query_source: row.get(12)?,
        })
    };

    let mut rows = Vec::new();
    if let Some(p) = provider {
        let mut stmt = conn.prepare(&sql)?;
        let it = stmt.query_map(params![since, until, p, limit], map_row)?;
        for r in it { rows.push(r?); }
    } else {
        let mut stmt = conn.prepare(&sql)?;
        let it = stmt.query_map(params![since, until, limit], map_row)?;
        for r in it { rows.push(r?); }
    }
    Ok(rows)
}

fn group_stats(conn: &Connection, key: &str, since: i64, until: i64) -> Result<Vec<(String, i64, i64, i64, String)>, AppError> {
    let sql = format!(
        "SELECT {key}, input_tokens, output_tokens, total_cost_usd
         FROM usage_records WHERE started_at >= ?1 AND started_at <= ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let it = stmt.query_map(params![since, until], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    let mut map: HashMap<String, (i64, i64, i64, Decimal)> = HashMap::new();
    for r in it {
        let (k, i, o, cost) = r?;
        let e = map.entry(k).or_insert((0, 0, 0, Decimal::ZERO));
        e.0 += 1;
        e.1 += i;
        e.2 += o;
        e.3 += Decimal::from_str(&cost).unwrap_or(Decimal::ZERO);
    }
    let mut out: Vec<(String, i64, i64, i64, String)> = map
        .into_iter()
        .map(|(k, (c, i, o, cost))| (k, c, i, o, cost.normalize().to_string()))
        .collect();
    // 按成本（Decimal）降序，避免字符串排序错误
    out.sort_by(|a, b| {
        let ca = Decimal::from_str(&a.4).unwrap_or(Decimal::ZERO);
        let cb = Decimal::from_str(&b.4).unwrap_or(Decimal::ZERO);
        cb.cmp(&ca)
    });
    Ok(out)
}

pub fn query_provider_stats(conn: &Connection, since: i64, until: i64) -> Result<Vec<ProviderStat>, AppError> {
    Ok(group_stats(conn, "provider_id", since, until)?
        .into_iter()
        .map(|(id, c, i, o, cost)| ProviderStat { provider_id: id, request_count: c, input_tokens: i, output_tokens: o, total_cost_usd: cost })
        .collect())
}

pub fn query_model_stats(conn: &Connection, since: i64, until: i64) -> Result<Vec<ModelStat>, AppError> {
    Ok(group_stats(conn, "model_id", since, until)?
        .into_iter()
        .map(|(id, c, i, o, cost)| ModelStat { model_id: id, request_count: c, input_tokens: i, output_tokens: o, total_cost_usd: cost })
        .collect())
}

pub fn get_overrides(conn: &Connection) -> Result<HashMap<String, ModelPricing>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT model_id, input_cost_per_million, output_cost_per_million,
                cache_read_cost_per_million, cache_creation_cost_per_million
         FROM pricing_overrides",
    )?;
    let it = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut map = HashMap::new();
    for r in it {
        let (id, i, o, cr, cc) = r?;
        if let Ok(p) = ModelPricing::from_strings(&i, &o, &cr, &cc) {
            map.insert(id, p);
        }
    }
    Ok(map)
}

pub fn set_override(conn: &Connection, model_id: &str, p: &ModelPricing) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO pricing_overrides (model_id, input_cost_per_million, output_cost_per_million,
            cache_read_cost_per_million, cache_creation_cost_per_million)
         VALUES (?1,?2,?3,?4,?5)
         ON CONFLICT(model_id) DO UPDATE SET
           input_cost_per_million = excluded.input_cost_per_million,
           output_cost_per_million = excluded.output_cost_per_million,
           cache_read_cost_per_million = excluded.cache_read_cost_per_million,
           cache_creation_cost_per_million = excluded.cache_creation_cost_per_million",
        params![
            model_id,
            p.input.to_string(), p.output.to_string(),
            p.cache_read.to_string(), p.cache_creation.to_string()
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::migrate;
    use crate::pricing::ModelPricing;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn conn() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&c).unwrap();
        c
    }

    fn rec(id: &str, model: &str, started: i64) -> UsageRecord {
        UsageRecord {
            request_id: id.into(),
            app_type: "zcode".into(),
            provider_id: "p1".into(),
            model_id: model.into(),
            input_tokens: 1000,
            output_tokens: 500,
            reasoning_tokens: 100,
            cache_read_tokens: 800,
            cache_creation_tokens: 0,
            input_cost_usd: "0.00006".into(),
            output_cost_usd: "0.0006".into(),
            cache_read_cost_usd: "0.0000048".into(),
            cache_creation_cost_usd: "0".into(),
            total_cost_usd: "0.0006648".into(),
            priced: true,
            started_at: started,
            duration_ms: Some(1000),
            first_token_ms: Some(200),
            status: "completed".into(),
            session_id: Some("s1".into()),
            query_source: Some("main_turn".into()),
            created_at: started,
        }
    }

    #[test]
    fn insert_is_idempotent() {
        let c = conn();
        assert!(insert_record(&c, &rec("a", "m1", 10)).unwrap());
        assert!(!insert_record(&c, &rec("a", "m1", 10)).unwrap());
    }

    #[test]
    fn summary_aggregates_tokens_and_cost() {
        let c = conn();
        insert_record(&c, &rec("a", "m1", 10)).unwrap();
        insert_record(&c, &rec("b", "m1", 20)).unwrap();
        let s = query_summary(&c, 0, 100).unwrap();
        assert_eq!(s.request_count, 2);
        assert_eq!(s.input_tokens, 2000);
        assert_eq!(s.output_tokens, 1000);
        assert_eq!(s.cache_read_tokens, 1600);
        assert_eq!(Decimal::from_str(&s.total_cost_usd).unwrap(),
                   Decimal::from_str("0.0013296").unwrap());
    }

    #[test]
    fn model_stats_groups_by_model() {
        let c = conn();
        insert_record(&c, &rec("a", "m1", 10)).unwrap();
        insert_record(&c, &rec("b", "m2", 20)).unwrap();
        let rows = query_model_stats(&c, 0, 100).unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn cursor_roundtrip() {
        let c = conn();
        assert!(get_cursor(&c, "src").unwrap().is_none());
        set_cursor(&c, "src", 123, 456, 789).unwrap();
        let cur = get_cursor(&c, "src").unwrap().unwrap();
        assert_eq!((cur.last_started_at, cur.last_mtime), (123, 456));
    }

    #[test]
    fn override_roundtrip() {
        let c = conn();
        let p = ModelPricing {
            input: Decimal::from_str("0.3").unwrap(),
            output: Decimal::from_str("1.2").unwrap(),
            cache_read: Decimal::from_str("0.006").unwrap(),
            cache_creation: Decimal::ZERO,
        };
        set_override(&c, "m1", &p).unwrap();
        let map = get_overrides(&c).unwrap();
        assert_eq!(map.get("m1").unwrap().input, p.input);
    }
}
