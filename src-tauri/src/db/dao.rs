use crate::error::AppError;
use crate::pricing::{override_lookup, ModelPricing, TieredPricing};
use crate::zcode::provider_names::ProviderName;
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
    /// 未定价行的 token 总量（input+output），用于成本可信度提示。
    pub unpriced_tokens: i64,
    /// 未定价的 (provider, model) 组合数。
    pub unpriced_models: i64,
}

/// 「供应商 + 模型」组合（用于核算范围筛选与清除范围的模型级选择）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ModelSel {
    pub provider_id: String,
    pub model_id: String,
}

/// 核算范围筛选（供应商级 + 模型级并集生效；两者都空 = 不筛选）。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ScopeFilter {
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default)]
    pub models: Vec<ModelSel>,
}

impl ScopeFilter {
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty() && self.models.is_empty()
    }
}

/// 生成供应商/模型范围的 WHERE 片段（以 `AND (...)` 开头，无筛选时为空串）。
/// 占位符编号从 `start`（1-based）开始连续分配，与返回的参数一一对应。
///
/// 语义：`provider_id IN (选中的供应商) OR (provider_id = ? AND model_id = ?)`，
/// 即供应商级与模型级选择取并集。
fn scope_condition(scope: &ScopeFilter, start: usize) -> (String, Vec<Value>) {
    if scope.is_empty() {
        return (String::new(), Vec::new());
    }
    let mut parts: Vec<String> = Vec::new();
    let mut values: Vec<Value> = Vec::new();
    let mut idx = start;
    if !scope.providers.is_empty() {
        let holders: Vec<String> = scope
            .providers
            .iter()
            .map(|p| {
                let s = format!("?{idx}");
                idx += 1;
                values.push(Value::from(p.clone()));
                s
            })
            .collect();
        parts.push(format!("provider_id IN ({})", holders.join(",")));
    }
    for m in &scope.models {
        parts.push(format!(
            "(provider_id = ?{idx} AND model_id = ?{})",
            idx + 1
        ));
        values.push(Value::from(m.provider_id.clone()));
        values.push(Value::from(m.model_id.clone()));
        idx += 2;
    }
    (format!(" AND ({})", parts.join(" OR ")), values)
}

/// 生成时间范围片段（闭区间），占位符编号从 `start` 开始；无限制时为空串。
fn time_condition(since: Option<i64>, until: Option<i64>, start: usize) -> (String, Vec<Value>) {
    let mut sql = String::new();
    let mut values = Vec::new();
    let mut idx = start;
    if since.is_some() {
        sql.push_str(&format!(" AND started_at >= ?{idx}"));
        values.push(Value::from(since.unwrap()));
        idx += 1;
    }
    if until.is_some() {
        sql.push_str(&format!(" AND started_at <= ?{idx}"));
        values.push(Value::from(until.unwrap()));
    }
    (sql, values)
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
    /// 计费档（"peak" | "off_peak"；None = 未启用峰谷的组合或未定价行）。
    /// 不存库，由 `annotate_price_tiers` 在查询后统一标注。
    pub price_tier: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStat {
    pub provider_id: String,
    pub request_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_cost_usd: String,
    pub unpriced_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelStat {
    pub model_id: String,
    pub request_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_cost_usd: String,
    pub unpriced_count: i64,
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

/// 已导入但尚未定价（`priced = 0`）的记录，用于定价稍后可用时重定价。
pub struct UnpricedRecord {
    pub request_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    /// 重定价按行判档（峰谷）需要请求开始时刻。
    pub started_at: i64,
}

pub fn query_unpriced_records(conn: &Connection) -> Result<Vec<UnpricedRecord>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT request_id, provider_id, model_id, input_tokens, output_tokens,
                cache_read_tokens, cache_creation_tokens, started_at
         FROM usage_records WHERE priced = 0",
    )?;
    let it = stmt.query_map([], |row| {
        Ok(UnpricedRecord {
            request_id: row.get(0)?,
            provider_id: row.get(1)?,
            model_id: row.get(2)?,
            input_tokens: row.get(3)?,
            output_tokens: row.get(4)?,
            cache_read_tokens: row.get(5)?,
            cache_creation_tokens: row.get(6)?,
            started_at: row.get(7)?,
        })
    })?;
    let mut out = Vec::new();
    for r in it { out.push(r?); }
    Ok(out)
}

/// 待重算成本的记录（覆盖重算用，不筛 `priced`）。
#[derive(Debug)]
pub struct RepriceRow {
    pub request_id: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    /// 重定价按行判档（峰谷）需要请求开始时刻。
    pub started_at: i64,
}

/// 取某 `(provider_id, model_id)` 组合的**全部**行（含已定价），供覆盖重算使用。
pub fn query_records_by_provider_model(
    conn: &Connection, provider_id: &str, model_id: &str,
) -> Result<Vec<RepriceRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT request_id, input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens, started_at
         FROM usage_records WHERE provider_id = ?1 AND model_id = ?2",
    )?;
    let it = stmt.query_map(params![provider_id, model_id], |row| {
        Ok(RepriceRow {
            request_id: row.get(0)?,
            input_tokens: row.get(1)?,
            output_tokens: row.get(2)?,
            cache_read_tokens: row.get(3)?,
            cache_creation_tokens: row.get(4)?,
            started_at: row.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for r in it { out.push(r?); }
    Ok(out)
}

/// 命中定价后回填成本并把 `priced` 置 1。
pub fn update_record_pricing(
    conn: &Connection, request_id: &str,
    input_cost_usd: &str, output_cost_usd: &str,
    cache_read_cost_usd: &str, cache_creation_cost_usd: &str,
    total_cost_usd: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE usage_records SET input_cost_usd=?2, output_cost_usd=?3,
            cache_read_cost_usd=?4, cache_creation_cost_usd=?5,
            total_cost_usd=?6, priced=1 WHERE request_id=?1",
        params![request_id, input_cost_usd, output_cost_usd,
                cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd],
    )?;
    Ok(())
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

pub fn query_summary(conn: &Connection, since: i64, until: i64, scope: Option<&ScopeFilter>) -> Result<Summary, AppError> {
    let empty = ScopeFilter::default();
    let scope = scope.unwrap_or(&empty);
    let (scope_sql, mut values) = scope_condition(scope, 3);
    values.insert(0, Value::from(until));
    values.insert(0, Value::from(since));
    let mut stmt = conn.prepare(&format!(
        "SELECT provider_id, model_id, input_tokens, output_tokens, reasoning_tokens,
                cache_read_tokens, cache_creation_tokens, session_id, total_cost_usd, priced
         FROM usage_records WHERE started_at >= ?1 AND started_at <= ?2{scope_sql}"
    ))?;
    let it = stmt.query_map(params_from_iter(values.iter()), |row| {
        Ok((
            row.get::<_, String>(0)?, row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?, row.get::<_, i64>(3)?, row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?, row.get::<_, i64>(6)?,
            row.get::<_, Option<String>>(7)?, row.get::<_, String>(8)?,
            row.get::<_, i64>(9)?,
        ))
    })?;

    let mut s = Summary::default();
    let mut total = Decimal::ZERO;
    let mut sessions = HashSet::new();
    let mut unpriced_models: HashSet<(String, String)> = HashSet::new();
    for r in it {
        let (p, m, i, o, re, cr, cc, sess, cost, priced) = r?;
        s.input_tokens += i;
        s.output_tokens += o;
        s.reasoning_tokens += re;
        s.cache_read_tokens += cr;
        s.cache_creation_tokens += cc;
        s.request_count += 1;
        if let Some(x) = sess { sessions.insert(x); }
        if priced == 0 {
            s.unpriced_count += 1;
            s.unpriced_tokens += i + o;
            unpriced_models.insert((p, m));
        }
        total += Decimal::from_str(&cost).unwrap_or(Decimal::ZERO);
    }
    s.session_count = sessions.len() as i64;
    s.unpriced_models = unpriced_models.len() as i64;
    s.total_cost_usd = total.normalize().to_string();
    Ok(s)
}

pub fn query_logs(conn: &Connection, since: i64, until: i64, scope: Option<&ScopeFilter>, limit: i64) -> Result<Vec<RequestLogRow>, AppError> {
    let empty = ScopeFilter::default();
    let scope = scope.unwrap_or(&empty);
    let (scope_sql, mut values) = scope_condition(scope, 3);
    values.insert(0, Value::from(until));
    values.insert(0, Value::from(since));
    let sql = format!(
        "SELECT request_id, provider_id, model_id, input_tokens, output_tokens,
                cache_read_tokens, total_cost_usd, priced, duration_ms, first_token_ms,
                status, started_at, query_source
         FROM usage_records WHERE started_at >= ?1 AND started_at <= ?2{scope_sql}
         ORDER BY started_at DESC LIMIT {limit}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let it = stmt.query_map(params_from_iter(values.iter()), |row| {
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
            price_tier: None,
        })
    })?;
    let mut rows = Vec::new();
    for r in it { rows.push(r?); }
    Ok(rows)
}

/// 请求日志的「计费档」标注：对已定价且其覆盖**启用峰谷**的行，按
/// `pricing::tier::is_peak(started_at)` 标注 "peak" / "off_peak"；其余行
/// （未定价、普通覆盖、表价）保持 None。
///
/// 匹配口径与 resolve 完全一致（统一走 `override_lookup` 归一化匹配），
/// 保证「按峰谷价计算」与「标注峰谷档」不会漂移。
pub fn annotate_price_tiers(
    rows: &mut [RequestLogRow],
    overrides: &HashMap<(String, String), TieredPricing>,
) {
    for r in rows.iter_mut() {
        if !r.priced {
            continue;
        }
        if let Some(tp) = override_lookup(&r.provider_id, &r.model_id, overrides) {
            if tp.peak.is_some() {
                r.price_tier = Some(
                    if crate::pricing::tier::is_peak(r.started_at) { "peak" } else { "off_peak" }
                        .to_string(),
                );
            }
        }
    }
}

fn group_stats(conn: &Connection, key: &str, since: i64, until: i64, scope: Option<&ScopeFilter>) -> Result<Vec<(String, i64, i64, i64, String, i64)>, AppError> {
    let empty = ScopeFilter::default();
    let scope = scope.unwrap_or(&empty);
    let (scope_sql, mut values) = scope_condition(scope, 3);
    values.insert(0, Value::from(until));
    values.insert(0, Value::from(since));
    let sql = format!(
        "SELECT {key}, input_tokens, output_tokens, total_cost_usd, priced
         FROM usage_records WHERE started_at >= ?1 AND started_at <= ?2{scope_sql}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let it = stmt.query_map(params_from_iter(values.iter()), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;

    let mut map: HashMap<String, (i64, i64, i64, Decimal, i64)> = HashMap::new();
    for r in it {
        let (k, i, o, cost, priced) = r?;
        let e = map.entry(k).or_insert((0, 0, 0, Decimal::ZERO, 0));
        e.0 += 1;
        e.1 += i;
        e.2 += o;
        e.3 += Decimal::from_str(&cost).unwrap_or(Decimal::ZERO);
        if priced == 0 {
            e.4 += 1;
        }
    }
    let mut out: Vec<(String, i64, i64, i64, String, i64)> = map
        .into_iter()
        .map(|(k, (c, i, o, cost, unpriced))| (k, c, i, o, cost.normalize().to_string(), unpriced))
        .collect();
    // 按成本（Decimal）降序，避免字符串排序错误
    out.sort_by(|a, b| {
        let ca = Decimal::from_str(&a.4).unwrap_or(Decimal::ZERO);
        let cb = Decimal::from_str(&b.4).unwrap_or(Decimal::ZERO);
        cb.cmp(&ca)
    });
    Ok(out)
}

pub fn query_provider_stats(conn: &Connection, since: i64, until: i64, scope: Option<&ScopeFilter>) -> Result<Vec<ProviderStat>, AppError> {
    Ok(group_stats(conn, "provider_id", since, until, scope)?
        .into_iter()
        .map(|(id, c, i, o, cost, unpriced)| ProviderStat {
            provider_id: id,
            request_count: c,
            input_tokens: i,
            output_tokens: o,
            total_cost_usd: cost,
            unpriced_count: unpriced,
        })
        .collect())
}

pub fn query_model_stats(conn: &Connection, since: i64, until: i64, scope: Option<&ScopeFilter>) -> Result<Vec<ModelStat>, AppError> {
    Ok(group_stats(conn, "model_id", since, until, scope)?
        .into_iter()
        .map(|(id, c, i, o, cost, unpriced)| ModelStat {
            model_id: id,
            request_count: c,
            input_tokens: i,
            output_tokens: o,
            total_cost_usd: cost,
            unpriced_count: unpriced,
        })
        .collect())
}

/// 供 UI 展示的覆盖行（单价以字符串原样返回；peak 四列为 None = 未启用峰谷）。
#[derive(Debug, Clone, Serialize)]
pub struct OverrideRow {
    pub provider_id: String,
    pub model_id: String,
    pub input: String,
    pub output: String,
    pub cache_read: String,
    pub cache_creation: String,
    pub peak_input: Option<String>,
    pub peak_output: Option<String>,
    pub peak_cache_read: Option<String>,
    pub peak_cache_creation: Option<String>,
    /// 该覆盖当前按归一化口径命中的记录数（与 resolve 的 override_lookup
    /// 完全同口径计算；0 表示覆盖不生效，前端应红字提示）。
    pub matched_count: i64,
}

/// 把覆盖行的原始单价字符串解析为 `TieredPricing`：空闲四价解析失败 → None
/// （该覆盖在 resolve 中不会生效）；peak 四列必须**全部**非 NULL 且解析成功
/// 才置 `peak = Some`，否则 None（防御半填 / 损坏数据）。
fn parse_tiered(
    i: &str, o: &str, cr: &str, cc: &str,
    pi: Option<&str>, po: Option<&str>, pcr: Option<&str>, pcc: Option<&str>,
) -> Option<TieredPricing> {
    let off_peak = ModelPricing::from_strings(i, o, cr, cc).ok()?;
    let peak = match (pi, po, pcr, pcc) {
        (Some(a), Some(b), Some(c), Some(d)) => ModelPricing::from_strings(a, b, c, d).ok(),
        _ => None,
    };
    Some(TieredPricing { off_peak, peak })
}

/// 覆盖 SELECT 的公共列（空闲四价 + peak 四价）。
const OVERRIDE_COLUMNS: &str =
    "provider_id, model_id, input_cost_per_million, output_cost_per_million,
     cache_read_cost_per_million, cache_creation_cost_per_million,
     peak_input_cost_per_million, peak_output_cost_per_million,
     peak_cache_read_cost_per_million, peak_cache_creation_cost_per_million";

type OverrideRaw = (
    String, String, String, String, String, String,
    Option<String>, Option<String>, Option<String>, Option<String>,
);

fn query_override_rows(conn: &Connection, order: bool) -> Result<Vec<OverrideRaw>, AppError> {
    let sql = format!(
        "SELECT {OVERRIDE_COLUMNS} FROM pricing_overrides{}",
        if order { " ORDER BY provider_id, model_id" } else { "" },
    );
    let mut stmt = conn.prepare(&sql)?;
    let it = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<String>>(9)?,
        ))
    })?;
    let mut out = Vec::new();
    for r in it { out.push(r?); }
    Ok(out)
}

pub fn get_overrides(conn: &Connection) -> Result<HashMap<(String, String), TieredPricing>, AppError> {
    let mut map = HashMap::new();
    for (pid, mid, i, o, cr, cc, pi, po, pcr, pcc) in query_override_rows(conn, false)? {
        if let Some(tp) = parse_tiered(&i, &o, &cr, &cc, pi.as_deref(), po.as_deref(), pcr.as_deref(), pcc.as_deref()) {
            map.insert((pid, mid), tp);
        }
    }
    Ok(map)
}

pub fn list_overrides(conn: &Connection) -> Result<Vec<OverrideRow>, AppError> {
    let mut out = Vec::new();
    for (pid, mid, i, o, cr, cc, pi, po, pcr, pcc) in query_override_rows(conn, true)? {
        // 命中数必须用与 resolve 相同的 override_lookup 计算（口径唯一）：
        // 以该覆盖为唯一覆盖项，判定该 provider 下每个 DISTINCT model_id 是否命中。
        let matched_count = match parse_tiered(
            &i, &o, &cr, &cc,
            pi.as_deref(), po.as_deref(), pcr.as_deref(), pcc.as_deref(),
        ) {
            Some(tp) => {
                let mut single = HashMap::new();
                single.insert((pid.clone(), mid.clone()), tp);
                count_matched_records(conn, &pid, &single)?
            }
            None => 0, // 单价损坏的覆盖不会在 resolve 中生效，视为 0 命中
        };
        out.push(OverrideRow {
            provider_id: pid,
            model_id: mid,
            input: i,
            output: o,
            cache_read: cr,
            cache_creation: cc,
            peak_input: pi,
            peak_output: po,
            peak_cache_read: pcr,
            peak_cache_creation: pcc,
            matched_count,
        });
    }
    Ok(out)
}

/// 某覆盖（以单覆盖 map 表达）按 `override_lookup` 口径当前命中的记录总数。
fn count_matched_records(
    conn: &Connection,
    provider_id: &str,
    single: &HashMap<(String, String), TieredPricing>,
) -> Result<i64, AppError> {
    let mut total = 0i64;
    for m in list_models_for_provider(conn, provider_id)? {
        if override_lookup(provider_id, &m, single).is_some() {
            let n: i64 = conn.query_row(
                "SELECT COUNT(*) FROM usage_records WHERE provider_id = ?1 AND model_id = ?2",
                params![provider_id, m],
                |r| r.get(0),
            )?;
            total += n;
        }
    }
    Ok(total)
}

/// 写入「供应商 + 模型」的覆盖。`peak` 为 Some 启用峰谷双档（高峰四价），
/// None 关闭峰谷（peak 列写 NULL，普通覆盖价保留）。
pub fn set_override(
    conn: &Connection, provider_id: &str, model_id: &str,
    p: &ModelPricing, peak: Option<&ModelPricing>,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO pricing_overrides (provider_id, model_id, input_cost_per_million, output_cost_per_million,
            cache_read_cost_per_million, cache_creation_cost_per_million,
            peak_input_cost_per_million, peak_output_cost_per_million,
            peak_cache_read_cost_per_million, peak_cache_creation_cost_per_million)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(provider_id, model_id) DO UPDATE SET
           input_cost_per_million = excluded.input_cost_per_million,
           output_cost_per_million = excluded.output_cost_per_million,
           cache_read_cost_per_million = excluded.cache_read_cost_per_million,
           cache_creation_cost_per_million = excluded.cache_creation_cost_per_million,
           peak_input_cost_per_million = excluded.peak_input_cost_per_million,
           peak_output_cost_per_million = excluded.peak_output_cost_per_million,
           peak_cache_read_cost_per_million = excluded.peak_cache_read_cost_per_million,
           peak_cache_creation_cost_per_million = excluded.peak_cache_creation_cost_per_million",
        params![
            provider_id, model_id,
            p.input.to_string(), p.output.to_string(),
            p.cache_read.to_string(), p.cache_creation.to_string(),
            peak.map(|x| x.input.to_string()),
            peak.map(|x| x.output.to_string()),
            peak.map(|x| x.cache_read.to_string()),
            peak.map(|x| x.cache_creation.to_string()),
        ],
    )?;
    Ok(())
}

pub fn delete_override(conn: &Connection, provider_id: &str, model_id: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM pricing_overrides WHERE provider_id = ?1 AND model_id = ?2",
        params![provider_id, model_id],
    )?;
    Ok(())
}

/// 某供应商下出现过的全部 DISTINCT `model_id`（按模型排序）。
/// 覆盖重算 / 删除清理 / 命中数统计共用：先用 `override_lookup` 判定命中再逐个取行。
pub fn list_models_for_provider(conn: &Connection, provider_id: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT model_id FROM usage_records WHERE provider_id = ?1 ORDER BY model_id",
    )?;
    let it = stmt.query_map(params![provider_id], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for r in it { out.push(r?); }
    Ok(out)
}

/// 清空某供应商下一组模型的成本并置 `priced = 0`，返回受影响行数。
/// 调用方负责先用 `override_lookup`（归一化口径）选出要清空的 model_id 集合，
/// 本函数只做清空，不含匹配逻辑。
pub fn clear_pricing_by_models(
    conn: &Connection, provider_id: &str, model_ids: &[String],
) -> Result<usize, AppError> {
    let mut total = 0usize;
    for m in model_ids {
        total += conn.execute(
            "UPDATE usage_records SET
                input_cost_usd='0', output_cost_usd='0',
                cache_read_cost_usd='0', cache_creation_cost_usd='0',
                total_cost_usd='0', priced=0
             WHERE provider_id = ?1 AND model_id = ?2",
            params![provider_id, m],
        )?;
    }
    Ok(total)
}

/// 删除「供应商 + 模型」的覆盖，并清空其影响范围内行的成本（置 `priced = 0`），
/// 返回清空的行数。
///
/// 影响范围用与 resolve 完全相同的 `override_lookup` 归一化口径确定：以被删覆盖
/// 为唯一覆盖项，判定该 provider 下每个 DISTINCT model_id 是否命中——避免
/// 「删了覆盖、成本还留着覆盖价」，也避免误清其它覆盖定价的行。
/// 删除后由调用方触发 sync 回填表价 / 未定价。
pub fn delete_override_and_clear(
    conn: &Connection, provider_id: &str, model_id: &str,
) -> Result<usize, AppError> {
    // 删除前先取该覆盖的价格，构造单覆盖 map 用于归一化匹配其影响范围。
    let all = get_overrides(conn)?;
    let mut removed: HashMap<(String, String), TieredPricing> = HashMap::new();
    if let Some(tp) = all.get(&(provider_id.to_string(), model_id.to_string())) {
        removed.insert((provider_id.to_string(), model_id.to_string()), tp.clone());
    }
    delete_override(conn, provider_id, model_id)?;
    if removed.is_empty() {
        return Ok(0); // 覆盖不存在或单价损坏（不会生效），无需清理
    }
    let models = list_models_for_provider(conn, provider_id)?;
    let matched: Vec<String> = models
        .into_iter()
        .filter(|m| override_lookup(provider_id, m, &removed).is_some())
        .collect();
    clear_pricing_by_models(conn, provider_id, &matched)
}

// ---------------------------------------------------------------------------
// 供应商名称快照（Issue P1-A：providerId → 可读名称）
// ---------------------------------------------------------------------------

/// 供 UI 展示的供应商名称行（`first_seen_at`/`updated_at` 不暴露给前端）。
#[derive(Debug, Clone, Serialize)]
pub struct ProviderNameRow {
    pub provider_id: String,
    pub display_name: String,
    pub source: String,
    pub base_url: Option<String>,
}

/// 快照供应商名称到自有库（只增不删：已从 ZCode 配置中消失的 provider
/// 绝不删除，其历史数据仍保留同步时捕获的名称）。
///
/// 冲突时更新 `display_name`/`base_url`/`updated_at`，`first_seen_at` 保留；
/// `source` 固定写 `zcode_config`。
pub fn upsert_provider_names(
    conn: &Connection,
    names: &[ProviderName],
) -> Result<(), AppError> {
    let now = chrono::Utc::now().timestamp_millis();
    for n in names {
        conn.execute(
            "INSERT INTO provider_names (provider_id, display_name, source, base_url, first_seen_at, updated_at)
             VALUES (?1, ?2, 'zcode_config', ?3, ?4, ?4)
             ON CONFLICT(provider_id) DO UPDATE SET
               display_name = excluded.display_name,
               base_url = excluded.base_url,
               updated_at = excluded.updated_at",
            params![n.provider_id, n.display_name, n.base_url, now],
        )?;
    }
    Ok(())
}

/// 全部已知的供应商名称（供前端把 provider_id 显示为可读名称）。
pub fn list_provider_names(conn: &Connection) -> Result<Vec<ProviderNameRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT provider_id, display_name, source, base_url
         FROM provider_names ORDER BY display_name, provider_id",
    )?;
    let it = stmt.query_map([], |row| {
        Ok(ProviderNameRow {
            provider_id: row.get(0)?,
            display_name: row.get(1)?,
            source: row.get(2)?,
            base_url: row.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for r in it { out.push(r?); }
    Ok(out)
}

// ---------------------------------------------------------------------------
// 供应商 > 模型层级（范围选择与清除弹窗共用）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ProviderModelRow {
    pub provider_id: String,
    pub model_id: String,
}

/// 库中出现过的全部 (provider, model) 组合，按供应商、模型排序。
pub fn list_provider_models(conn: &Connection) -> Result<Vec<ProviderModelRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT provider_id, model_id FROM usage_records ORDER BY provider_id, model_id",
    )?;
    let it = stmt.query_map([], |row| {
        Ok(ProviderModelRow {
            provider_id: row.get(0)?,
            model_id: row.get(1)?,
        })
    })?;
    let mut out = Vec::new();
    for r in it { out.push(r?); }
    Ok(out)
}

// ---------------------------------------------------------------------------
// 未定价模型清单（成本可信度警告的明细）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct UnpricedModelRow {
    pub provider_id: String,
    pub model_id: String,
    pub request_count: i64,
    /// input + output（与成本同口径的 token 总量）。
    pub total_tokens: i64,
    pub first_started_at: i64,
    pub last_started_at: i64,
    /// 估算成本区间（USD 字符串）：按同供应商已定价模型的每 token 单价范围推算；
    /// 该供应商无可参照的已定价行时为 None（前端显示「无法估算」）。
    pub est_cost_low_usd: Option<String>,
    pub est_cost_high_usd: Option<String>,
}

/// 未定价 (provider, model) 分组明细 + 粗略成本估算区间。
///
/// 估算口径：单价 = total_cost / (input+output)，取同供应商已定价行的
/// [最低, 最高] 单价 × 该组合未定价 token 量。仅供参考，非精确计算。
pub fn query_unpriced_models(conn: &Connection, since: i64, until: i64, scope: Option<&ScopeFilter>) -> Result<Vec<UnpricedModelRow>, AppError> {
    let empty = ScopeFilter::default();
    let scope = scope.unwrap_or(&empty);
    let (scope_sql, mut values) = scope_condition(scope, 3);
    values.insert(0, Value::from(until));
    values.insert(0, Value::from(since));
    let mut stmt = conn.prepare(&format!(
        "SELECT provider_id, model_id, priced, input_tokens, output_tokens,
                total_cost_usd, started_at
         FROM usage_records WHERE started_at >= ?1 AND started_at <= ?2{scope_sql}"
    ))?;
    let it = stmt.query_map(params_from_iter(values.iter()), |row| {
        Ok((
            row.get::<_, String>(0)?, row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?, row.get::<_, i64>(3)?, row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?, row.get::<_, i64>(6)?,
        ))
    })?;

    struct Agg { count: i64, tokens: i64, first: i64, last: i64 }
    let mut unpriced: HashMap<(String, String), Agg> = HashMap::new();
    // 每供应商已定价行的每 token 单价范围（cost / (input+output)）
    let mut unit_range: HashMap<String, (Decimal, Decimal)> = HashMap::new();
    for r in it {
        let (p, m, priced, i, o, cost, started) = r?;
        let tokens = i + o;
        if priced == 0 {
            let e = unpriced.entry((p, m)).or_insert(Agg { count: 0, tokens: 0, first: started, last: started });
            e.count += 1;
            e.tokens += tokens;
            e.first = e.first.min(started);
            e.last = e.last.max(started);
        } else if tokens > 0 {
            let unit = Decimal::from_str(&cost).unwrap_or(Decimal::ZERO) / Decimal::from(tokens);
            let e = unit_range.entry(p).or_insert((unit, unit));
            e.0 = e.0.min(unit);
            e.1 = e.1.max(unit);
        }
    }

    let mut out: Vec<UnpricedModelRow> = unpriced
        .into_iter()
        .map(|((p, m), a)| {
            let est: Option<(Decimal, Decimal)> =
                unit_range.get(&p).copied().map(|(min, max)| {
                    (Decimal::from(a.tokens) * min, Decimal::from(a.tokens) * max)
                });
            UnpricedModelRow {
                provider_id: p,
                model_id: m,
                request_count: a.count,
                total_tokens: a.tokens,
                first_started_at: a.first,
                last_started_at: a.last,
                est_cost_low_usd: est.map(|(l, _)| l.normalize().to_string()),
                est_cost_high_usd: est.map(|(_, h)| h.normalize().to_string()),
            }
        })
        .collect();
    out.sort_by(|a, b| a.provider_id.cmp(&b.provider_id).then(a.model_id.cmp(&b.model_id)));
    Ok(out)
}

// ---------------------------------------------------------------------------
// 清除用量数据（危险操作：删除 + 审计 + 墓碑）
// ---------------------------------------------------------------------------

/// 清除范围（时间段 + 供应商/模型并集）。全空 = 清除全部。
#[derive(Debug, Clone, Deserialize)]
pub struct ClearScope {
    pub since: Option<i64>,
    pub until: Option<i64>,
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default)]
    pub models: Vec<ModelSel>,
}

impl ClearScope {
    /// 是否为「无任何条件」的全清。
    fn is_full_clear(&self) -> bool {
        self.since.is_none() && self.until.is_none()
            && self.providers.is_empty() && self.models.is_empty()
    }
}

/// 清除前预览：将删除的条数与授权确认短语。
#[derive(Debug, Clone, Serialize)]
pub struct ClearPreview {
    pub deleted_count: i64,
    pub confirm_token: String,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub providers: Vec<String>,
    pub models: Vec<ModelSel>,
}

/// 清除结果反馈。
#[derive(Debug, Clone, Serialize)]
pub struct ClearResult {
    pub deleted_count: i64,
    pub affected_since: Option<i64>,
    pub affected_until: Option<i64>,
    pub affected_providers: Vec<String>,
    pub affected_models: Vec<ModelSel>,
    pub audit_id: i64,
    /// 是否为全清（连带重置了同步游标）。
    pub cursor_reset: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditLogRow {
    pub id: i64,
    pub actor: String,
    pub action: String,
    pub started_at_from: Option<i64>,
    pub started_at_to: Option<i64>,
    pub providers: Vec<String>,
    pub models: Vec<ModelSel>,
    pub deleted_count: i64,
    pub created_at: i64,
}

/// 同步回灌过滤用的清除墓碑（闭区间 [ts_from, ts_to]）。
#[derive(Debug, Clone)]
pub struct Tombstone {
    pub ts_from: i64,
    pub ts_to: i64,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
}

/// 授权确认短语：`删除N`（N 为预览到的删除条数）。执行时后端强制校验，
/// 预览后数据变化（条数不符）会自然校验失败，防止误删。
pub fn confirm_token(count: i64) -> String {
    format!("删除{count}")
}

struct ClearTargets {
    count: i64,
    min_started: Option<i64>,
    max_started: Option<i64>,
    providers: Vec<String>,
    models: Vec<ModelSel>,
}

/// 清除条件的完整 WHERE 片段（时间 + 范围，均可选），占位符从 `start` 编号。
fn clear_condition(scope: &ClearScope, start: usize) -> (String, Vec<Value>) {
    let (t_sql, mut values) = time_condition(scope.since, scope.until, start);
    let next = start + values.len();
    let (s_sql, s_values) = {
        let f = ScopeFilter {
            providers: scope.providers.clone(),
            models: scope.models.clone(),
        };
        scope_condition(&f, next)
    };
    values.extend(s_values);
    (format!("{t_sql}{s_sql}"), values)
}

/// 校验清除范围的边界：时间倒置、供应商不存在、模型不存在/不属于所选供应商。
pub fn validate_clear_scope(conn: &Connection, scope: &ClearScope) -> Result<(), AppError> {
    if let (Some(s), Some(u)) = (scope.since, scope.until) {
        if s > u {
            return Err(AppError::Config(format!(
                "开始时间（{}）晚于结束时间（{}），请修正时间范围",
                chrono::DateTime::from_timestamp_millis(s).map(|d| d.to_string()).unwrap_or_else(|| s.to_string()),
                chrono::DateTime::from_timestamp_millis(u).map(|d| d.to_string()).unwrap_or_else(|| u.to_string()),
            )));
        }
    }
    if scope.providers.is_empty() && scope.models.is_empty() {
        return Ok(()); // 全清或仅按时间清除，无供应商/模型可校验
    }
    let known_providers: HashSet<String> = {
        let mut stmt = conn.prepare("SELECT DISTINCT provider_id FROM usage_records")?;
        let it = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut set = HashSet::new();
        for r in it { set.insert(r?); }
        set
    };
    for p in &scope.providers {
        if !known_providers.contains(p) {
            return Err(AppError::Config(format!("供应商「{p}」不存在（数据库中没有该供应商的用量记录）")));
        }
    }
    let known_models: HashSet<(String, String)> = {
        let mut stmt = conn.prepare("SELECT DISTINCT provider_id, model_id FROM usage_records")?;
        let it = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut set = HashSet::new();
        for r in it { set.insert(r?); }
        set
    };
    for m in &scope.models {
        let key = (m.provider_id.clone(), m.model_id.clone());
        if !known_models.contains(&key) {
            let belongs_elsewhere = known_models.iter().any(|(_, mm)| mm == &m.model_id);
            return Err(AppError::Config(if belongs_elsewhere {
                format!("模型「{}」不属于供应商「{}」（或该组合无数据），请检查选择", m.model_id, m.provider_id)
            } else {
                format!("模型「{}」不存在（数据库中没有该模型的用量记录）", m.model_id)
            }));
        }
    }
    Ok(())
}

/// 计算将被清除的目标集合（条数、时间边界、供应商、模型）。
/// 目标为空时返回明确错误，杜绝「静默的成功」。
fn clear_targets(conn: &Connection, scope: &ClearScope) -> Result<ClearTargets, AppError> {
    let (cond, values) = clear_condition(scope, 1);
    let (count, min_started, max_started): (i64, Option<i64>, Option<i64>) = conn.query_row(
        &format!(
            "SELECT COUNT(*), MIN(started_at), MAX(started_at) FROM usage_records WHERE 1=1{cond}"
        ),
        params_from_iter(values.iter()),
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if count == 0 {
        return Err(AppError::Config("所选范围内没有数据，无需清除".into()));
    }
    let providers: Vec<String> = {
        let mut stmt = conn.prepare(&format!(
            "SELECT DISTINCT provider_id FROM usage_records WHERE 1=1{cond} ORDER BY provider_id"
        ))?;
        let it = stmt.query_map(params_from_iter(values.iter()), |r| r.get::<_, String>(0))?;
        let mut v = Vec::new();
        for r in it { v.push(r?); }
        v
    };
    let models: Vec<ModelSel> = {
        let mut stmt = conn.prepare(&format!(
            "SELECT DISTINCT provider_id, model_id FROM usage_records WHERE 1=1{cond} ORDER BY provider_id, model_id"
        ))?;
        let it = stmt.query_map(params_from_iter(values.iter()), |r| {
            Ok(ModelSel { provider_id: r.get(0)?, model_id: r.get(1)? })
        })?;
        let mut v = Vec::new();
        for r in it { v.push(r?); }
        v
    };
    Ok(ClearTargets { count, min_started, max_started, providers, models })
}

/// 预览清除范围（不删除）。返回条数与授权确认短语。
pub fn preview_clear(conn: &Connection, scope: &ClearScope) -> Result<ClearPreview, AppError> {
    validate_clear_scope(conn, scope)?;
    let t = clear_targets(conn, scope)?;
    Ok(ClearPreview {
        deleted_count: t.count,
        confirm_token: confirm_token(t.count),
        since: scope.since,
        until: scope.until,
        providers: t.providers,
        models: t.models,
    })
}

/// 执行清除：校验 → 事务内（计数 → 授权校验 → 删除 → 写墓碑 → 全清重置游标 → 审计）。
///
/// 墓碑记录被删范围，供 sync 过滤 ZCode 源库回灌的行（重叠窗口会把
/// `started_at` 早于水位线的行再次导入；源库只读，删除只发生在自有库）。
pub fn clear_records(conn: &mut Connection, scope: &ClearScope, confirm: &str, actor: &str) -> Result<ClearResult, AppError> {
    validate_clear_scope(conn, scope)?;
    let tx = conn.transaction()?;
    let t = clear_targets(&tx, scope)?;

    let expected = confirm_token(t.count);
    if confirm.trim() != expected {
        return Err(AppError::Config(format!(
            "授权确认不通过：请输入「{expected}」以确认。若预览后数据已变化，请重新预览"
        )));
    }

    let (cond, values) = clear_condition(scope, 1);
    let deleted = tx.execute(
        &format!("DELETE FROM usage_records WHERE 1=1{cond}"),
        params_from_iter(values.iter()),
    )?;

    // 写墓碑：时间边界缺省为 [0, now]（不限过去时间 = 挡住清除时刻前的全部相关行）。
    let now = chrono::Utc::now().timestamp_millis();
    let (ts_from, ts_to) = match (scope.since, scope.until) {
        (Some(s), Some(u)) => (s, u),
        (Some(s), None) => (s, now),
        (None, Some(u)) => (0, u),
        (None, None) => (0, now),
    };
    let mut tomb_rows: Vec<(i64, i64, Option<String>, Option<String>)> = Vec::new();
    if scope.providers.is_empty() && scope.models.is_empty() {
        tomb_rows.push((ts_from, ts_to, None, None));
    } else {
        for p in &scope.providers {
            tomb_rows.push((ts_from, ts_to, Some(p.clone()), None));
        }
        for m in &scope.models {
            tomb_rows.push((ts_from, ts_to, Some(m.provider_id.clone()), Some(m.model_id.clone())));
        }
    }
    for (f, to, p, m) in &tomb_rows {
        tx.execute(
            "INSERT INTO clear_tombstones (ts_from, ts_to, provider_id, model_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![f, to, p, m, now],
        )?;
    }

    // 全清（无任何条件）时重置同步游标：下次 sync 从头扫描（结果被墓碑全部过滤），
    // 游标不再指向已删除的数据。
    let cursor_reset = scope.is_full_clear();
    if cursor_reset {
        tx.execute("DELETE FROM sync_cursors", [])?;
    }

    let audit_id = insert_audit_log(
        &tx, actor, "clear_usage", scope.since, scope.until,
        &scope.providers, &scope.models, deleted as i64,
    )?;
    tx.commit()?;

    Ok(ClearResult {
        deleted_count: deleted as i64,
        affected_since: t.min_started,
        affected_until: t.max_started,
        affected_providers: t.providers,
        affected_models: t.models,
        audit_id,
        cursor_reset,
    })
}

pub fn insert_audit_log(
    conn: &Connection, actor: &str, action: &str,
    since: Option<i64>, until: Option<i64>,
    providers: &[String], models: &[ModelSel], deleted_count: i64,
) -> Result<i64, AppError> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut stmt = conn.prepare(
        "INSERT INTO audit_log (actor, action, started_at_from, started_at_to, providers, models, deleted_count, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    stmt.execute(params![
        actor, action, since, until,
        serde_json::to_string(providers).unwrap_or_else(|_| "[]".into()),
        serde_json::to_string(models).unwrap_or_else(|_| "[]".into()),
        deleted_count, now,
    ])?;
    Ok(conn.last_insert_rowid())
}

pub fn list_audit_logs(conn: &Connection, limit: i64) -> Result<Vec<AuditLogRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, actor, action, started_at_from, started_at_to, providers, models, deleted_count, created_at
         FROM audit_log ORDER BY id DESC LIMIT ?1",
    )?;
    let it = stmt.query_map(params![limit], |row| {
        Ok((
            row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?,
            row.get::<_, Option<i64>>(3)?, row.get::<_, Option<i64>>(4)?,
            row.get::<_, String>(5)?, row.get::<_, String>(6)?,
            row.get::<_, i64>(7)?, row.get::<_, i64>(8)?,
        ))
    })?;
    let mut out = Vec::new();
    for r in it {
        let (id, actor, action, from, to, providers_json, models_json, deleted_count, created_at) = r?;
        let providers: Vec<String> = serde_json::from_str(&providers_json)
            .map_err(|e| AppError::Database(format!("审计日志 providers 字段损坏: {e}")))?;
        let models: Vec<ModelSel> = serde_json::from_str(&models_json)
            .map_err(|e| AppError::Database(format!("审计日志 models 字段损坏: {e}")))?;
        out.push(AuditLogRow { id, actor, action, started_at_from: from, started_at_to: to, providers, models, deleted_count, created_at });
    }
    Ok(out)
}

pub fn list_tombstones(conn: &Connection) -> Result<Vec<Tombstone>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT ts_from, ts_to, provider_id, model_id FROM clear_tombstones",
    )?;
    let it = stmt.query_map([], |row| {
        Ok(Tombstone {
            ts_from: row.get(0)?,
            ts_to: row.get(1)?,
            provider_id: row.get(2)?,
            model_id: row.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for r in it { out.push(r?); }
    Ok(out)
}

/// 行是否命中任一墓碑（命中 = 已被用户清除，同步时必须跳过）。
pub fn is_tombstoned(started_at: i64, provider_id: &str, model_id: &str, tombs: &[Tombstone]) -> bool {
    tombs.iter().any(|t| {
        started_at >= t.ts_from
            && started_at <= t.ts_to
            && t.provider_id.as_deref().map_or(true, |p| p == provider_id)
            && t.model_id.as_deref().map_or(true, |m| m == model_id)
    })
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
        let s = query_summary(&c, 0, 100, None).unwrap();
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
        let rows = query_model_stats(&c, 0, 100, None).unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn group_stats_counts_unpriced() {
        let c = conn();
        insert_record(&c, &rec("a", "m1", 10)).unwrap();
        let mut unpriced = rec("b", "m1", 20);
        unpriced.priced = false;
        insert_record(&c, &unpriced).unwrap();

        let models = query_model_stats(&c, 0, 100, None).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].request_count, 2);
        assert_eq!(models[0].unpriced_count, 1);

        let providers = query_provider_stats(&c, 0, 100, None).unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].request_count, 2);
        assert_eq!(providers[0].unpriced_count, 1);
    }

    #[test]
    fn summary_and_stats_filter_by_scope() {
        let c = conn();
        let mut b = rec("b", "m2", 20);
        b.provider_id = "p2".into();
        insert_record(&c, &rec("a", "m1", 10)).unwrap();
        insert_record(&c, &b).unwrap();

        // 供应商级筛选：只含 p2
        let scope_p2 = ScopeFilter { providers: vec!["p2".into()], models: vec![] };
        let s = query_summary(&c, 0, 100, Some(&scope_p2)).unwrap();
        assert_eq!(s.request_count, 1);
        assert_eq!(s.input_tokens, 1000);
        assert_eq!(query_summary(&c, 0, 100, None).unwrap().request_count, 2);
        let nope = ScopeFilter { providers: vec!["nope".into()], models: vec![] };
        assert_eq!(query_summary(&c, 0, 100, Some(&nope)).unwrap().request_count, 0);

        let providers = query_provider_stats(&c, 0, 100, Some(&scope_p2)).unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider_id, "p2");
        assert!(query_provider_stats(&c, 0, 100, Some(&nope)).unwrap().is_empty());

        let models = query_model_stats(&c, 0, 100, Some(&scope_p2)).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model_id, "m2");
        assert!(query_model_stats(&c, 0, 100, Some(&nope)).unwrap().is_empty());
    }

    /// 范围筛选支持多供应商 + 跨供应商多选模型，且并集生效。
    #[test]
    fn scope_filter_supports_multi_providers_and_cross_provider_models() {
        let c = conn();
        for (id, p, m) in [("a", "p1", "m1"), ("b", "p1", "m2"), ("c", "p2", "m3"), ("d", "p3", "m4")] {
            let mut r = rec(id, m, 10);
            r.provider_id = p.into();
            insert_record(&c, &r).unwrap();
        }
        // 供应商 p1（全部）+ (p2, m3) 单模型 → a/b/c
        let scope = ScopeFilter {
            providers: vec!["p1".into()],
            models: vec![ModelSel { provider_id: "p2".into(), model_id: "m3".into() }],
        };
        let s = query_summary(&c, 0, 100, Some(&scope)).unwrap();
        assert_eq!(s.request_count, 3);

        // 跨供应商多选模型：(p1,m2) + (p3,m4) → b/d
        let scope2 = ScopeFilter {
            providers: vec![],
            models: vec![
                ModelSel { provider_id: "p1".into(), model_id: "m2".into() },
                ModelSel { provider_id: "p3".into(), model_id: "m4".into() },
            ],
        };
        let logs = query_logs(&c, 0, 100, Some(&scope2), 100).unwrap();
        let mut ids: Vec<&str> = logs.iter().map(|l| l.request_id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["b", "d"]);

        // 供应商级与模型级重叠时不重复计数（并集语义）
        let scope3 = ScopeFilter {
            providers: vec!["p1".into()],
            models: vec![ModelSel { provider_id: "p1".into(), model_id: "m1".into() }],
        };
        assert_eq!(query_summary(&c, 0, 100, Some(&scope3)).unwrap().request_count, 2);
    }

    #[test]
    fn summary_counts_unpriced_tokens_and_models() {
        let c = conn();
        insert_record(&c, &rec("a", "m1", 10)).unwrap();
        let mut u1 = rec("b", "m1", 20);
        u1.priced = false;
        let mut u2 = rec("c", "m2", 30);
        u2.priced = false;
        u2.provider_id = "p2".into();
        insert_record(&c, &u1).unwrap();
        insert_record(&c, &u2).unwrap();

        let s = query_summary(&c, 0, 100, None).unwrap();
        assert_eq!(s.unpriced_count, 2);
        assert_eq!(s.unpriced_tokens, (1000 + 500) * 2);
        assert_eq!(s.unpriced_models, 2, "两个 (provider, model) 组合未定价");
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
    fn override_roundtrip_keyed_by_provider_and_model() {
        let c = conn();
        let p = ModelPricing {
            input: Decimal::from_str("0.3").unwrap(),
            output: Decimal::from_str("1.2").unwrap(),
            cache_read: Decimal::from_str("0.006").unwrap(),
            cache_creation: Decimal::ZERO,
        };
        set_override(&c, "p1", "m1", &p, None).unwrap();
        // 同一模型不同供应商是两条独立记录
        let p2 = ModelPricing { input: Decimal::from_str("9").unwrap(), ..p.clone() };
        set_override(&c, "p2", "m1", &p2, None).unwrap();

        let map = get_overrides(&c).unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map.get(&("p1".into(), "m1".into())).unwrap().off_peak.input, p.input);
        assert_eq!(map.get(&("p2".into(), "m1".into())).unwrap().off_peak.input, p2.input);
        assert!(map.values().all(|tp| tp.peak.is_none()), "未启用峰谷的覆盖 peak 应为 None");

        // UPSERT：同键再写覆盖单价
        let p3 = ModelPricing { input: Decimal::from_str("0.5").unwrap(), ..p.clone() };
        set_override(&c, "p1", "m1", &p3, None).unwrap();
        let map = get_overrides(&c).unwrap();
        assert_eq!(map.len(), 2, "UPSERT 不应新增行");
        assert_eq!(map.get(&("p1".into(), "m1".into())).unwrap().off_peak.input, p3.input);
    }

    #[test]
    fn list_and_delete_override() {
        let c = conn();
        let p = ModelPricing {
            input: Decimal::from_str("0.3").unwrap(),
            output: Decimal::from_str("1.2").unwrap(),
            cache_read: Decimal::from_str("0.006").unwrap(),
            cache_creation: Decimal::ZERO,
        };
        set_override(&c, "p2", "m2", &p, None).unwrap();
        set_override(&c, "p1", "m1", &p, None).unwrap();

        let rows = list_overrides(&c).unwrap();
        assert_eq!(rows.len(), 2);
        // 按 (provider_id, model_id) 排序
        assert_eq!((rows[0].provider_id.as_str(), rows[0].model_id.as_str()), ("p1", "m1"));
        assert_eq!(rows[0].input, "0.3");
        assert_eq!(rows[0].cache_read, "0.006");

        delete_override(&c, "p1", "m1").unwrap();
        let rows = list_overrides(&c).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider_id, "p2");
        // 删除不存在的键不报错
        delete_override(&c, "nope", "nope").unwrap();
    }

    /// `clear_pricing_by_models`：只清指定供应商下指定模型的行，其它供应商/模型不受影响。
    #[test]
    fn clear_pricing_by_models_clears_targets_and_keeps_others() {
        let c = conn();
        let mut a = rec("a", "m1", 10);
        a.provider_id = "p1".into();
        let mut b = rec("b", "m1", 20);
        b.provider_id = "p1".into();
        let mut other = rec("c", "m1", 30);
        other.provider_id = "p2".into();
        insert_record(&c, &a).unwrap();
        insert_record(&c, &b).unwrap();
        insert_record(&c, &other).unwrap();

        let n = clear_pricing_by_models(&c, "p1", &["m1".to_string()]).unwrap();
        assert_eq!(n, 2, "应命中 p1/m1 共 2 行");

        let logs = query_logs(&c, 0, i64::MAX, None, 100).unwrap();
        for id in ["a", "b"] {
            let row = logs.iter().find(|l| l.request_id == id).unwrap();
            assert!(!row.priced, "{id} 应变为未定价");
            assert_eq!(row.total_cost_usd, "0");
        }
        // 其它供应商不受影响
        let other_row = logs.iter().find(|l| l.request_id == "c").unwrap();
        assert!(other_row.priced);
        assert_ne!(other_row.total_cost_usd, "0");

        // 已 priced=0 的行再次清空仍计入（UPDATE 匹配行数）；空列表清 0 行
        assert_eq!(clear_pricing_by_models(&c, "p1", &["m1".to_string()]).unwrap(), 2);
        assert_eq!(clear_pricing_by_models(&c, "p1", &[]).unwrap(), 0);
        assert_eq!(clear_pricing_by_models(&c, "p1", &["nope".to_string()]).unwrap(), 0);
    }

    /// 删除覆盖按归一化口径清理：命名空间行与裸名行都被清空，其它模型不动。
    #[test]
    fn delete_override_and_clear_uses_normalized_matching() {
        let c = conn();
        let mut x = rec("x", "deepseek-ai/DeepSeek-V4.1-flash", 10);
        x.provider_id = "p1".into();
        let mut y = rec("y", "deepseek-v4.1-flash", 20);
        y.provider_id = "p1".into();
        let mut z = rec("z", "m2", 30);
        z.provider_id = "p1".into();
        let mut other_p = rec("w", "deepseek-v4.1-flash", 40);
        other_p.provider_id = "p2".into();
        insert_record(&c, &x).unwrap();
        insert_record(&c, &y).unwrap();
        insert_record(&c, &z).unwrap();
        insert_record(&c, &other_p).unwrap();

        set_override(&c, "p1", "deepseek-v4.1-flash", &override_price(), None).unwrap();
        // 列表视图的命中数应为 2（x + y，跨命名空间同口径）
        let rows = list_overrides(&c).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].matched_count, 2, "覆盖应按归一化口径命中 2 行");

        let n = delete_override_and_clear(&c, "p1", "deepseek-v4.1-flash").unwrap();
        assert_eq!(n, 2, "命名空间行与裸名行都应被清空");
        assert!(get_overrides(&c).unwrap().is_empty(), "覆盖行已删除");

        let logs = query_logs(&c, 0, i64::MAX, None, 100).unwrap();
        for id in ["x", "y"] {
            let row = logs.iter().find(|l| l.request_id == id).unwrap();
            assert!(!row.priced, "{id} 应变为未定价");
            assert_eq!(row.total_cost_usd, "0");
        }
        // 其它模型与其它供应商不受影响
        let z_row = logs.iter().find(|l| l.request_id == "z").unwrap();
        assert!(z_row.priced, "其它模型不受影响");
        let w_row = logs.iter().find(|l| l.request_id == "w").unwrap();
        assert!(w_row.priced, "其它供应商不受影响");
    }

    /// 删除不存在的覆盖：清 0 行、不报错（成本不可能来自一个不生效的覆盖）。
    #[test]
    fn delete_override_and_clear_noop_for_missing_override() {
        let c = conn();
        insert_record(&c, &rec("a", "m1", 10)).unwrap();
        let n = delete_override_and_clear(&c, "p1", "m1").unwrap();
        assert_eq!(n, 0);
        let row = &query_logs(&c, 0, i64::MAX, None, 100).unwrap()[0];
        assert!(row.priced, "无覆盖可删时不得清空既有成本");
    }

    // -- P1-A：供应商名称快照 -------------------------------------------------

    #[test]
    fn upsert_provider_names_is_idempotent_and_keeps_gone_providers() {
        let c = conn();
        let names = vec![
            pname("p1", "Provider One"),
            pname("p2", "Provider Two"),
        ];
        upsert_provider_names(&c, &names).unwrap();
        upsert_provider_names(&c, &names_iter(&c)).unwrap(); // 幂等：重复写同样数据
        assert_eq!(list_provider_names(&c).unwrap().len(), 2, "重复 upsert 不应新增行");

        // p2 从配置中消失 → 仍保留（只增不删）
        upsert_provider_names(&c, &[pname("p1", "Provider One")]).unwrap();
        let rows = list_provider_names(&c).unwrap();
        assert_eq!(rows.len(), 2, "已消失的 provider 绝不删除");
        assert!(rows.iter().any(|r| r.provider_id == "p2" && r.display_name == "Provider Two"));
    }

    #[test]
    fn upsert_provider_names_updates_rename_and_keeps_first_seen() {
        let c = conn();
        upsert_provider_names(&c, &[pname("p1", "Old Name")]).unwrap();
        // 伪造时间戳，验证 first_seen_at 保留、updated_at 更新
        c.execute(
            "UPDATE provider_names SET first_seen_at = 100, updated_at = 200 WHERE provider_id = 'p1'",
            [],
        ).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(5));
        upsert_provider_names(&c, &[pname("p1", "New Name")]).unwrap();

        let rows = list_provider_names(&c).unwrap();
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.display_name, "New Name", "改名后 display_name 应更新");
        assert_eq!(r.source, "zcode_config");
        let (first, updated): (i64, i64) = c.query_row(
            "SELECT first_seen_at, updated_at FROM provider_names WHERE provider_id = 'p1'",
            [], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(first, 100, "first_seen_at 必须保留");
        assert!(updated > 200, "updated_at 应被更新为本次 upsert 时间");
    }

    #[test]
    fn list_provider_names_orders_and_roundtrips() {
        let c = conn();
        let mut n1 = pname("b-id", "Zeta");
        n1.base_url = Some("https://example.com/v1".into());
        upsert_provider_names(&c, &[n1, pname("a-id", "Alpha")]).unwrap();
        let rows = list_provider_names(&c).unwrap();
        assert_eq!(rows.len(), 2);
        // 按 display_name 排序
        assert_eq!((rows[0].provider_id.as_str(), rows[0].display_name.as_str()), ("a-id", "Alpha"));
        assert_eq!(rows[1].provider_id, "b-id");
        assert_eq!(rows[1].base_url.as_deref(), Some("https://example.com/v1"));
    }

    fn override_price() -> ModelPricing {
        ModelPricing {
            input: Decimal::from_str("0.15").unwrap(),
            output: Decimal::from_str("0.6").unwrap(),
            cache_read: Decimal::from_str("0.003").unwrap(),
            cache_creation: Decimal::ZERO,
        }
    }

    fn pname(id: &str, name: &str) -> ProviderName {
        ProviderName {
            provider_id: id.into(),
            display_name: name.into(),
            base_url: None,
        }
    }

    /// 辅助：把当前表内 provider_names 重复 upsert 一遍（取回再写，验证幂等）。
    fn names_iter(c: &rusqlite::Connection) -> Vec<ProviderName> {
        list_provider_names(c).unwrap()
            .into_iter()
            .map(|r| ProviderName {
                provider_id: r.provider_id,
                display_name: r.display_name,
                base_url: r.base_url,
            })
            .collect()
    }

    #[test]
    fn query_records_by_provider_model_returns_all_rows_of_combo() {
        let c = conn();
        let mut a = rec("a", "m1", 10);
        a.provider_id = "p1".into();
        a.priced = true;
        let mut b = rec("b", "m1", 20);
        b.provider_id = "p1".into();
        b.priced = false; // 未定价行也必须返回
        let mut other = rec("c", "m1", 30);
        other.provider_id = "p2".into();
        insert_record(&c, &a).unwrap();
        insert_record(&c, &b).unwrap();
        insert_record(&c, &other).unwrap();

        let rows = query_records_by_provider_model(&c, "p1", "m1").unwrap();
        let mut ids: Vec<&str> = rows.iter().map(|r| r.request_id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["a", "b"], "只取目标组合的所有行（含 priced=0）");
        // started_at 供覆盖重算按行判档（峰谷）
        let a = rows.iter().find(|r| r.request_id == "a").unwrap();
        let b = rows.iter().find(|r| r.request_id == "b").unwrap();
        assert_eq!((a.started_at, b.started_at), (10, 20));

        let empty = query_records_by_provider_model(&c, "p1", "nope").unwrap();
        assert!(empty.is_empty());
    }

    /// 组合清除：时间段 + 供应商 + 模型，只删命中行；结果反馈与审计日志完整。
    #[test]
    fn clear_records_combined_scope_deletes_and_audits() {
        let mut c = conn();
        for (id, p, m, started) in
            [("a", "p1", "m1", 100), ("b", "p1", "m2", 200), ("c", "p2", "m1", 300), ("d", "p1", "m1", 500)]
        {
            let mut r = rec(id, m, started);
            r.provider_id = p.into();
            insert_record(&c, &r).unwrap();
        }
        // 预览：p1 的 m1，时间 [0, 400] → 只命中 a
        let scope = ClearScope {
            since: Some(0), until: Some(400),
            providers: vec![],
            models: vec![ModelSel { provider_id: "p1".into(), model_id: "m1".into() }],
        };
        let pv = preview_clear(&c, &scope).unwrap();
        assert_eq!(pv.deleted_count, 1);
        assert_eq!(pv.confirm_token, "删除1");
        assert_eq!(pv.models.len(), 1);

        // 授权短语错误 → 拒绝且不删除
        let err = clear_records(&mut c, &scope, "删除2", "tester").unwrap_err();
        assert!(err.to_string().contains("授权确认"), "实际错误: {err}");
        assert_eq!(query_summary(&c, 0, i64::MAX, None).unwrap().request_count, 4);

        // 正确短语 → 删除 a；反馈与审计完整
        let res = clear_records(&mut c, &scope, "删除1", "tester").unwrap();
        assert_eq!(res.deleted_count, 1);
        assert_eq!(res.affected_since, Some(100));
        assert_eq!(res.affected_until, Some(100));
        assert_eq!(res.affected_providers, vec!["p1".to_string()]);
        assert_eq!(res.affected_models.len(), 1);
        assert!(!res.cursor_reset);

        let s = query_summary(&c, 0, i64::MAX, None).unwrap();
        assert_eq!(s.request_count, 3, "a 已删除，其余保留");

        let logs = list_audit_logs(&c, 10).unwrap();
        assert_eq!(logs.len(), 1);
        let a = &logs[0];
        assert_eq!(a.actor, "tester");
        assert_eq!(a.action, "clear_usage");
        assert_eq!((a.started_at_from, a.started_at_to), (Some(0), Some(400)));
        assert_eq!(a.models.len(), 1);
        assert_eq!(a.providers.len(), 0, "模型级选择不等于供应商级");
        assert_eq!(a.deleted_count, 1);
    }

    /// 清除边界：时间倒置 / 供应商不存在 / 模型不属于供应商 / 范围无数据 / 模型不存在。
    #[test]
    fn clear_records_validates_boundary_conditions() {
        let c = conn();
        insert_record(&c, &rec("a", "m1", 100)).unwrap();

        // 开始时间晚于结束时间
        let bad_time = ClearScope { since: Some(200), until: Some(100), providers: vec![], models: vec![] };
        let err = preview_clear(&c, &bad_time).unwrap_err();
        assert!(err.to_string().contains("晚于结束时间"), "实际错误: {err}");

        // 供应商不存在
        let no_provider = ClearScope {
            since: None, until: None,
            providers: vec!["ghost".into()], models: vec![],
        };
        let err = preview_clear(&c, &no_provider).unwrap_err();
        assert!(err.to_string().contains("供应商「ghost」不存在"), "实际错误: {err}");

        // 模型存在但不属于所选供应商（m1 只在 p1 下）
        let wrong_owner = ClearScope {
            since: None, until: None,
            providers: vec![],
            models: vec![ModelSel { provider_id: "p2".into(), model_id: "m1".into() }],
        };
        let err = preview_clear(&c, &wrong_owner).unwrap_err();
        assert!(err.to_string().contains("不属于供应商"), "实际错误: {err}");

        // 模型完全不存在
        let no_model = ClearScope {
            since: None, until: None,
            providers: vec![],
            models: vec![ModelSel { provider_id: "p1".into(), model_id: "nope".into() }],
        };
        let err = preview_clear(&c, &no_model).unwrap_err();
        assert!(err.to_string().contains("不存在"), "实际错误: {err}");

        // 范围内无数据（时间窗口错开）
        let empty_range = ClearScope { since: Some(999999), until: Some(1000000), providers: vec![], models: vec![] };
        let err = preview_clear(&c, &empty_range).unwrap_err();
        assert!(err.to_string().contains("没有数据"), "实际错误: {err}");
    }

    /// 全清：删除全部行、重置游标、写全域墓碑；审计记录供应商与模型为空（= 不限）。
    #[test]
    fn clear_records_full_clear_resets_cursor() {
        let mut c = conn();
        insert_record(&c, &rec("a", "m1", 100)).unwrap();
        set_cursor(&c, "src", 100, 7, 9).unwrap();

        let scope = ClearScope { since: None, until: None, providers: vec![], models: vec![] };
        let pv = preview_clear(&c, &scope).unwrap();
        assert_eq!(pv.deleted_count, 1);
        let res = clear_records(&mut c, &scope, "删除1", "tester").unwrap();
        assert_eq!(res.deleted_count, 1);
        assert!(res.cursor_reset);
        assert!(get_cursor(&c, "src").unwrap().is_none(), "全清后游标应被重置");

        // 全域墓碑：覆盖 [0, now]
        let tombs = list_tombstones(&c).unwrap();
        assert_eq!(tombs.len(), 1);
        assert_eq!(tombs[0].ts_from, 0);
        assert!(tombs[0].provider_id.is_none() && tombs[0].model_id.is_none());
        assert!(is_tombstoned(50, "p1", "m1", &tombs));

        let a = &list_audit_logs(&c, 10).unwrap()[0];
        assert_eq!(a.deleted_count, 1);
        assert_eq!((a.started_at_from, a.started_at_to), (None, None));
        assert!(a.providers.is_empty() && a.models.is_empty(), "全清时供应商/模型列表为空 = 不限");
    }

    /// 部分清除写的墓碑必须拦下命中行、放行其它行（sync 防回灌的 DAO 侧前提）。
    #[test]
    fn tombstones_filter_matching_rows_only() {
        let mut c = conn();
        insert_record(&c, &rec("a", "m1", 100)).unwrap();
        let scope = ClearScope {
            since: Some(0), until: Some(150),
            providers: vec!["p1".into()], models: vec![],
        };
        clear_records(&mut c, &scope, "删除1", "tester").unwrap();

        let tombs = list_tombstones(&c).unwrap();
        assert!(is_tombstoned(100, "p1", "m1", &tombs), "区间内的行被拦");
        assert!(is_tombstoned(100, "p1", "m2", &tombs), "供应商级墓碑拦该供应商全部模型");
        assert!(!is_tombstoned(100, "p2", "m1", &tombs), "其它供应商不受影响");
        assert!(!is_tombstoned(999, "p1", "m1", &tombs), "区间外不拦");
    }

    /// 未定价清单：按 (provider, model) 分组，估算区间取同供应商已定价行的
    /// 每 token 单价范围；无可参照时为 None。
    #[test]
    fn query_unpriced_models_groups_and_estimates() {
        let c = conn();
        // p1 已定价参照：a=1500 tokens 0.0003 → 单价 0.0000002；b=1000 tokens 0.0006 → 单价 0.0000006
        let mut a = rec("a", "m1", 10);
        a.input_tokens = 1000; a.output_tokens = 500; a.total_cost_usd = "0.0003".into();
        let mut b = rec("b", "m1", 20);
        b.input_tokens = 1000; b.output_tokens = 0; b.total_cost_usd = "0.0006".into();
        // p1 未定价：u1（同模型）与 u2
        let mut u1 = rec("u1", "m1", 30);
        u1.priced = false; u1.total_cost_usd = "0".into();
        let mut u2 = rec("u2", "m2", 40);
        u2.priced = false; u2.total_cost_usd = "0".into(); u2.provider_id = "p1".into();
        // p2 未定价：该供应商无已定价参照 → 无法估算
        let mut u3 = rec("u3", "m1", 50);
        u3.priced = false; u3.total_cost_usd = "0".into(); u3.provider_id = "p2".into();
        for r in [&a, &b, &u1, &u2, &u3] { insert_record(&c, r).unwrap(); }

        let rows = query_unpriced_models(&c, 0, i64::MAX, None).unwrap();
        assert_eq!(rows.len(), 3, "p1/m1、p1/m2、p2/m1 三个未定价组合");

        let p1m1 = rows.iter().find(|r| r.provider_id == "p1" && r.model_id == "m1").unwrap();
        assert_eq!(p1m1.request_count, 1);
        assert_eq!(p1m1.total_tokens, 1500);
        // 估算区间 = 1500 × [0.0000002, 0.0000006] = [0.0003, 0.0009]
        let lo = Decimal::from_str(p1m1.est_cost_low_usd.as_ref().unwrap()).unwrap();
        let hi = Decimal::from_str(p1m1.est_cost_high_usd.as_ref().unwrap()).unwrap();
        assert_eq!(lo, Decimal::from_str("0.0003").unwrap());
        assert_eq!(hi, Decimal::from_str("0.0009").unwrap());

        let p2m1 = rows.iter().find(|r| r.provider_id == "p2").unwrap();
        assert!(p2m1.est_cost_low_usd.is_none() && p2m1.est_cost_high_usd.is_none(),
                "无可参照已定价行时应为 None（无法估算）");

        // 范围筛选跟随：只看 p2 → 只剩 p2 的未定价组合
        let scope = ScopeFilter { providers: vec!["p2".into()], models: vec![] };
        let rows = query_unpriced_models(&c, 0, i64::MAX, Some(&scope)).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider_id, "p2");
    }

    #[test]
    fn list_provider_models_returns_distinct_sorted_pairs() {
        let c = conn();
        for (id, p, m) in [("a", "p2", "m1"), ("b", "p1", "m2"), ("c", "p1", "m1"), ("d", "p1", "m1")] {
            let mut r = rec(id, m, 10);
            r.provider_id = p.into();
            insert_record(&c, &r).unwrap();
        }
        let rows = list_provider_models(&c).unwrap();
        let pairs: Vec<(String, String)> = rows.into_iter().map(|r| (r.provider_id, r.model_id)).collect();
        assert_eq!(pairs, vec![
            ("p1".into(), "m1".into()), ("p1".into(), "m2".into()), ("p2".into(), "m1".into()),
        ], "去重且按 provider、model 排序");
    }

    // -- 峰谷定价（DeepSeek 峰谷覆盖）----------------------------------------

    /// 高峰档价（空闲价 ×2 的官方参考口径）。
    fn peak_price() -> ModelPricing {
        ModelPricing {
            input: Decimal::from_str("0.3").unwrap(),
            output: Decimal::from_str("1.2").unwrap(),
            cache_read: Decimal::from_str("0.006").unwrap(),
            cache_creation: Decimal::ZERO,
        }
    }

    #[test]
    fn set_override_with_peak_roundtrip_and_disable() {
        let c = conn();
        let off = override_price();
        let peak = peak_price();

        // 启用峰谷：存取往返，两组价各自精确还原
        set_override(&c, "p1", "m1", &off, Some(&peak)).unwrap();
        let got = get_overrides(&c).unwrap().get(&("p1".into(), "m1".into())).cloned().unwrap();
        assert_eq!(got.off_peak, off);
        assert_eq!(got.peak, Some(peak));

        // 关闭峰谷：peak 列写 NULL，普通覆盖价必须保留
        set_override(&c, "p1", "m1", &off, None).unwrap();
        let got = get_overrides(&c).unwrap().get(&("p1".into(), "m1".into())).cloned().unwrap();
        assert_eq!(got.off_peak, off, "关闭峰谷后普通覆盖价必须保留");
        assert_eq!(got.peak, None, "关闭峰谷后 peak 必须为 None");
    }

    /// 半填数据防御：peak 列部分 NULL 或解析失败 → peak 回落 None（未启用峰谷）。
    #[test]
    fn get_overrides_tolerates_half_filled_peak_columns() {
        let c = conn();
        let off = override_price();
        set_override(&c, "p1", "m1", &off, Some(&peak_price())).unwrap();

        // 部分 peak 列为 NULL
        c.execute("UPDATE pricing_overrides SET peak_output_cost_per_million = NULL", []).unwrap();
        let got = get_overrides(&c).unwrap().get(&("p1".into(), "m1".into())).cloned().unwrap();
        assert_eq!(got.off_peak, off, "空闲组不受 peak 半填影响");
        assert_eq!(got.peak, None, "peak 列半填 NULL 必须回落为未启用峰谷");

        // peak 列损坏（解析失败）
        c.execute("UPDATE pricing_overrides SET peak_output_cost_per_million = 'not-a-number'", []).unwrap();
        let got = get_overrides(&c).unwrap().get(&("p1".into(), "m1".into())).cloned().unwrap();
        assert_eq!(got.peak, None, "peak 值解析失败必须回落为未启用峰谷");
    }

    #[test]
    fn list_overrides_exposes_peak_columns() {
        let c = conn();
        let off = override_price();
        set_override(&c, "p1", "m1", &off, Some(&peak_price())).unwrap();
        let rows = list_overrides(&c).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].peak_input.as_deref(), Some("0.3"));
        assert_eq!(rows[0].peak_output.as_deref(), Some("1.2"));
        assert_eq!(rows[0].peak_cache_read.as_deref(), Some("0.006"));
        assert_eq!(rows[0].peak_cache_creation.as_deref(), Some("0"));
        assert_eq!(rows[0].matched_count, 0, "无用量记录时命中数为 0");

        // 关闭峰谷 → peak 四列回 NULL
        set_override(&c, "p1", "m1", &off, None).unwrap();
        let rows = list_overrides(&c).unwrap();
        assert!(
            rows[0].peak_input.is_none() && rows[0].peak_output.is_none()
                && rows[0].peak_cache_read.is_none() && rows[0].peak_cache_creation.is_none(),
            "关闭峰谷后 peak 列应全部为 None"
        );
    }

    /// 日志行工厂（annotate 测试用）。
    fn log_row(id: &str, provider: &str, model: &str, started: i64, priced: bool) -> RequestLogRow {
        RequestLogRow {
            request_id: id.into(),
            provider_id: provider.into(),
            model_id: model.into(),
            input_tokens: 1000,
            output_tokens: 0,
            cache_read_tokens: 0,
            total_cost_usd: if priced { "0.00015".into() } else { "0".into() },
            priced,
            duration_ms: None,
            first_token_ms: None,
            status: "completed".into(),
            started_at: started,
            query_source: None,
            price_tier: None,
        }
    }

    /// 计费档标注：仅「启用峰谷且已定价」的组合被标注，且按行判档；
    /// 普通覆盖与未定价行保持 None。匹配走 override_lookup（归一化口径）。
    #[test]
    fn annotate_price_tiers_marks_only_peak_enabled_combo() {
        // 2026-10-12 为周一（节假日表外的普通工作日），days_from_civil = 20738
        const DAY: i64 = 86_400_000;
        let peak_ts = 20_738 * DAY + 2 * 3_600_000;  // 北京 10:00
        let off_ts = 20_738 * DAY + 11 * 3_600_000;  // 北京 19:00

        let mut rows = vec![
            log_row("a", "p1", "m1", peak_ts, true),
            log_row("b", "p1", "m1", off_ts, true),
            log_row("c", "p1", "m2", peak_ts, true),
            log_row("d", "p1", "m1", peak_ts, false),
        ];
        assert!(crate::pricing::tier::is_peak(peak_ts), "测试前提：peak_ts 应判为峰档");
        assert!(!crate::pricing::tier::is_peak(off_ts), "测试前提：off_ts 应判为谷档");

        let mut overrides = HashMap::new();
        overrides.insert(
            ("p1".to_string(), "m1".to_string()),
            TieredPricing { off_peak: override_price(), peak: Some(peak_price()) },
        );
        // 普通覆盖（未启用峰谷）
        overrides.insert(
            ("p1".to_string(), "m2".to_string()),
            TieredPricing { off_peak: override_price(), peak: None },
        );
        // 裸名峰谷覆盖：供命名空间记录归一化命中
        overrides.insert(
            ("p1".to_string(), "deepseek-v4.1-flash".to_string()),
            TieredPricing { off_peak: override_price(), peak: Some(peak_price()) },
        );

        annotate_price_tiers(&mut rows, &overrides);
        assert_eq!(rows[0].price_tier.as_deref(), Some("peak"), "峰段行应标注 peak");
        assert_eq!(rows[1].price_tier.as_deref(), Some("off_peak"), "谷段行应标注 off_peak");
        assert_eq!(rows[2].price_tier, None, "普通覆盖（未启用峰谷）不标注");
        assert_eq!(rows[3].price_tier, None, "未定价行不标注");

        // 覆盖键为裸名、记录为命名空间名：同口径命中并标注
        let mut ns_rows = vec![log_row("e", "p1", "deepseek-ai/DeepSeek-V4.1-flash", peak_ts, true)];
        annotate_price_tiers(&mut ns_rows, &overrides);
        assert_eq!(ns_rows[0].price_tier.as_deref(), Some("peak"), "命名空间记录应经归一化命中并标注");

        // 空 overrides：全部不标注
        let mut plain = vec![log_row("f", "p1", "m1", peak_ts, true)];
        annotate_price_tiers(&mut plain, &HashMap::new());
        assert_eq!(plain[0].price_tier, None, "无覆盖时不得标注");
    }
}
