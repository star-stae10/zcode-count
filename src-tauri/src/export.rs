//! 请求日志导出（CSV / JSON）。
//!
//! 字段与「请求日志」页签的 10 列一致：时间、供应商、计费模型、输入、输出、
//! 总成本、计费档、用时/首字、状态、来源。供应商取 provider_names 的显示名
//! （无映射回退原始 ID，与 UI 的 `providerLabel` 同口径）；用时/首字与
//! `RequestLogTable` 的渲染文本同格式。格式差异：CSV 面向人/Excel（中文
//! 列头与档位、UTF-8 BOM、CRLF），JSON 面向程序（英文键、机器值
//! "peak"/"off_peak"、RFC 3339 时间、null 语义）。

use crate::db::dao::RequestLogRow;
use serde::Serialize;
use std::collections::HashMap;

/// 导出格式（由前端保存对话框决定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Json,
}

impl ExportFormat {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "csv" => Ok(Self::Csv),
            "json" => Ok(Self::Json),
            other => Err(format!("不支持的导出格式「{other}」（仅支持 csv / json）")),
        }
    }
}

/// JSON 导出行（字段名稳定，供外部脚本消费）。
#[derive(Debug, Serialize)]
struct ExportRow {
    /// 请求开始时间（本地时区，RFC 3339 含偏移，如 2026-09-28T17:22:35+08:00）。
    time: String,
    /// 供应商显示名（无映射 = 原始 provider_id）。
    provider: String,
    model: String,
    input_tokens: i64,
    output_tokens: i64,
    /// 未定价行为 null（对应 UI 的「—」）。
    total_cost_usd: Option<String>,
    /// "peak" | "off_peak"；null = 未启用峰谷覆盖或未定价（与后端 price_tier 标注一致）。
    price_tier: Option<String>,
    /// 用时/首字文本（如 "9.7s / 3.0s"；无用时为 "—"）。
    duration_text: String,
    status: String,
    /// 查询来源（main_turn / subagent / …；无则 null）。
    source: Option<String>,
}

const CSV_HEADERS: [&str; 10] = [
    "时间", "供应商", "计费模型", "输入", "输出", "总成本", "计费档", "用时/首字", "状态", "来源",
];

/// 供应商显示名：有映射返回名称，无映射回退原始 ID（与 UI 同口径）。
fn provider_display(names: &HashMap<String, String>, provider_id: &str) -> String {
    names
        .get(provider_id)
        .cloned()
        .unwrap_or_else(|| provider_id.to_string())
}

/// 毫秒时间戳 → 本地时间（异常时间戳兜底为 Unix 纪元，导出不因此中断）。
fn local_time(ms: i64) -> chrono::DateTime<chrono::Local> {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or(chrono::DateTime::UNIX_EPOCH)
        .with_timezone(&chrono::Local)
}

/// 用时/首字文本，与 `RequestLogTable` 的渲染逻辑保持一致：
/// 无用时为 "—"；有首字时以 " / " 拼接。
fn duration_text(duration_ms: Option<i64>, first_token_ms: Option<i64>) -> String {
    let Some(d) = duration_ms else {
        return "—".to_string();
    };
    let mut s = format!("{:.1}s", d as f64 / 1000.0);
    if let Some(f) = first_token_ms {
        s.push_str(&format!(" / {:.1}s", f as f64 / 1000.0));
    }
    s
}

/// CSV 字段转义：含逗号/引号/换行的字段用引号包裹，内部引号翻倍。
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv_fields(r: &RequestLogRow, names: &HashMap<String, String>) -> [String; 10] {
    [
        local_time(r.started_at).format("%Y-%m-%d %H:%M:%S").to_string(),
        provider_display(names, &r.provider_id),
        r.model_id.clone(),
        r.input_tokens.to_string(),
        r.output_tokens.to_string(),
        r.priced.then(|| r.total_cost_usd.clone()).unwrap_or_default(),
        match r.price_tier.as_deref() {
            Some("peak") => "峰".to_string(),
            Some("off_peak") => "谷".to_string(),
            _ => String::new(),
        },
        duration_text(r.duration_ms, r.first_token_ms),
        r.status.clone(),
        r.query_source.clone().unwrap_or_default(),
    ]
}

/// 生成 CSV 文本（UTF-8 BOM + CRLF，Excel 直接打开中文不乱码）。
pub fn to_csv(rows: &[RequestLogRow], names: &HashMap<String, String>) -> String {
    let mut out = String::from("\u{FEFF}");
    out.push_str(&CSV_HEADERS.iter().map(|h| csv_escape(h)).collect::<Vec<_>>().join(","));
    out.push_str("\r\n");
    for r in rows {
        let fields = csv_fields(r, names);
        out.push_str(&fields.iter().map(|f| csv_escape(f)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    out
}

/// 生成 JSON 文本（pretty 数组；空数据为 `[]`）。
pub fn to_json(rows: &[RequestLogRow], names: &HashMap<String, String>) -> String {
    let export_rows: Vec<ExportRow> = rows
        .iter()
        .map(|r| ExportRow {
            time: local_time(r.started_at).to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
            provider: provider_display(names, &r.provider_id),
            model: r.model_id.clone(),
            input_tokens: r.input_tokens,
            output_tokens: r.output_tokens,
            total_cost_usd: r.priced.then(|| r.total_cost_usd.clone()),
            price_tier: r.price_tier.clone(),
            duration_text: duration_text(r.duration_ms, r.first_token_ms),
            status: r.status.clone(),
            source: r.query_source.clone(),
        })
        .collect();
    serde_json::to_string_pretty(&export_rows).unwrap_or_else(|_| "[]".into())
}

/// 按格式渲染导出内容。
pub fn render(format: ExportFormat, rows: &[RequestLogRow], names: &HashMap<String, String>) -> String {
    match format {
        ExportFormat::Csv => to_csv(rows, names),
        ExportFormat::Json => to_json(rows, names),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> RequestLogRow {
        RequestLogRow {
            request_id: "req-1".into(),
            provider_id: "p1".into(),
            model_id: "deepseek-v4.1-flash".into(),
            input_tokens: 157_378,
            output_tokens: 953,
            cache_read_tokens: 145_024,
            total_cost_usd: "0.00572".into(),
            priced: true,
            duration_ms: Some(9700),
            first_token_ms: Some(3000),
            status: "completed".into(),
            started_at: 1_790_000_000_000,
            query_source: Some("main_turn".into()),
            price_tier: Some("peak".into()),
        }
    }

    fn names() -> HashMap<String, String> {
        HashMap::from([("p1".to_string(), "OpenCode Go (Chat)".to_string())])
    }

    #[test]
    fn csv_renders_ui_aligned_columns() {
        let csv = to_csv(&[row()], &names());
        // BOM + 中文表头（与请求日志列一一对应）
        assert!(csv.starts_with('\u{FEFF}'));
        assert_eq!(
            csv.lines().next().unwrap(),
            "\u{FEFF}时间,供应商,计费模型,输入,输出,总成本,计费档,用时/首字,状态,来源"
        );
        let data = csv.lines().nth(1).unwrap();
        // 供应商用显示名；档位/耗时与 UI 同文本
        assert!(data.contains("OpenCode Go (Chat)"));
        assert!(data.contains(",峰,"));
        assert!(data.contains("0.00572"));
        assert!(data.contains("157378"));
        assert!(data.contains("9.7s / 3.0s"));
        assert!(data.contains("completed"));
        assert!(data.contains("main_turn"));
        // 本地时间 YYYY-MM-DD HH:MM:SS
        assert!(
            data.split(',').next().unwrap().matches(char::is_numeric).count() >= 12,
            "时间字段应为日期时间格式: {data}"
        );
    }

    #[test]
    fn csv_unpriced_and_missing_fields_render_empty_or_dash() {
        let mut r = row();
        r.priced = false;
        r.price_tier = None;
        r.duration_ms = None;
        r.first_token_ms = None;
        r.query_source = None;
        r.provider_id = "p2".into(); // 无名称映射 → 回退原始 ID
        let data = to_csv(&[r], &names());
        let fields: Vec<&str> = data.lines().nth(1).unwrap().split(',').collect();
        assert_eq!(fields[1], "p2", "无映射供应商回退原始 ID");
        assert_eq!(fields[5], "", "未定价成本为空（对应 UI 的 —）");
        assert_eq!(fields[6], "", "无档位为空");
        assert_eq!(fields[7], "—", "无用时显示 —");
        assert_eq!(fields[9], "", "无来源为空");
    }

    #[test]
    fn csv_escapes_special_characters() {
        let mut r = row();
        r.model_id = "a,b\"c\nd".into();
        let data = to_csv(&[r], &names());
        assert!(data.contains("\"a,b\"\"c\nd\""), "含逗号/引号/换行的字段应被引号包裹并转义引号: {data}");
    }

    #[test]
    fn json_roundtrip_with_machine_values_and_nulls() {
        let priced = to_json(&[row()], &names());
        let v: serde_json::Value = serde_json::from_str(&priced).unwrap();
        let r0 = &v.as_array().unwrap()[0];
        assert_eq!(r0["provider"], "OpenCode Go (Chat)");
        assert_eq!(r0["model"], "deepseek-v4.1-flash");
        assert_eq!(r0["input_tokens"], 157_378);
        assert_eq!(r0["output_tokens"], 953);
        assert_eq!(r0["total_cost_usd"], "0.00572");
        assert_eq!(r0["price_tier"], "peak");
        assert_eq!(r0["duration_text"], "9.7s / 3.0s");
        assert_eq!(r0["status"], "completed");
        assert_eq!(r0["source"], "main_turn");
        assert!(r0["time"].as_str().unwrap().contains('T'), "时间应为 RFC 3339");

        let mut r = row();
        r.priced = false;
        r.price_tier = None;
        r.query_source = None;
        let v: serde_json::Value = serde_json::from_str(&to_json(&[r], &names())).unwrap();
        let r0 = &v.as_array().unwrap()[0];
        assert!(r0["total_cost_usd"].is_null(), "未定价成本应为 null");
        assert!(r0["price_tier"].is_null());
        assert!(r0["source"].is_null());
    }

    #[test]
    fn render_rejects_unknown_format() {
        assert_eq!(ExportFormat::parse("csv").unwrap(), ExportFormat::Csv);
        assert_eq!(ExportFormat::parse("json").unwrap(), ExportFormat::Json);
        assert!(ExportFormat::parse("xlsx").is_err());
    }
}
