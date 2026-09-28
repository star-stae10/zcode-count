pub mod candidates;
pub mod cost;
pub mod table;
pub mod tier;

use candidates::model_candidates;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct ModelPricing {
    pub input: Decimal,
    pub output: Decimal,
    pub cache_read: Decimal,
    pub cache_creation: Decimal,
}

impl ModelPricing {
    pub fn from_strings(i: &str, o: &str, cr: &str, cc: &str) -> Result<Self, rust_decimal::Error> {
        Ok(Self {
            input: i.parse()?,
            output: o.parse()?,
            cache_read: cr.parse()?,
            cache_creation: cc.parse()?,
        })
    }
}

/// 分档单价：空闲档必填，高峰档可选（None = 未启用峰谷）。
///
/// - 表价命中 → `peak = None`（cc-switch 表价无峰谷概念）；
/// - 覆盖命中 → 带出该覆盖的高峰价（未启用峰谷的覆盖同样 `peak = None`）。
#[derive(Debug, Clone, PartialEq)]
pub struct TieredPricing {
    pub off_peak: ModelPricing,
    pub peak: Option<ModelPricing>,
}

impl TieredPricing {
    /// 按行 `started_at`（Unix 毫秒）选档；高峰档未配置时恒回落空闲档
    /// （防御半配置数据，保证任何时刻都有可用单价）。
    pub fn pick(&self, started_at_ms: i64) -> &ModelPricing {
        match &self.peak {
            Some(p) if tier::is_peak(started_at_ms) => p,
            _ => &self.off_peak,
        }
    }
}

/// cc-switch 定价库路径：%USERPROFILE%\.cc-switch\cc-switch.db
pub fn cc_switch_db_path() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    PathBuf::from(home).join(".cc-switch").join("cc-switch.db")
}

/// 覆盖查价：先 `(provider_id, model_id)` 精确，再按 `model_candidates` 归一化候选匹配。
///
/// 匹配口径与表价（`table.lookup`）统一为同一套 `model_candidates` 规则；
/// `provider_id` 始终精确匹配（禁止跨供应商命中）。覆盖键归一化后无候选
/// （如 `unknown`）→ 跳过该覆盖，不误伤其它模型。
///
/// 本函数是覆盖匹配的**唯一**入口：resolve、覆盖重算、删除清理、命中数统计、
/// 请求日志的计费档标注全部经此判定，禁止第二套匹配。
pub fn override_lookup<'a>(
    provider_id: &str,
    model_id: &str,
    overrides: &'a HashMap<(String, String), TieredPricing>,
) -> Option<&'a TieredPricing> {
    // ① 精确优先。
    if let Some(p) = overrides.get(&(provider_id.to_string(), model_id.to_string())) {
        return Some(p);
    }
    // ② 对该 provider 下的覆盖键建「归一化候选 → 价格」索引。覆盖项按覆盖
    //    model_id 排序后再入索引，多个覆盖键归一化到同一候选时行为确定
    //    （字典序最小者生效）。
    let mut scoped: Vec<(&str, &TieredPricing)> = overrides
        .iter()
        .filter(|((p, _), _)| p == provider_id)
        .map(|((_, m), v)| (m.as_str(), v))
        .collect();
    scoped.sort_by(|a, b| a.0.cmp(b.0));
    let mut index: HashMap<String, &TieredPricing> = HashMap::new();
    for (om, p) in &scoped {
        for c in model_candidates(om) {
            index.entry(c).or_insert(*p);
        }
    }
    // ③ 按记录 model_id 的候选顺序逐个查（原始归一化形态优先于去后缀形态）。
    for c in model_candidates(model_id) {
        if let Some(p) = index.get(&c) {
            return Some(p);
        }
    }
    None
}

/// 解析单价：`(provider_id, model_id)` 的覆盖优先于 cc-switch 定价表。
///
/// 返回按档结构：表价命中 → `peak = None`；覆盖命中 → 带出该覆盖的高峰价。
pub fn resolve(
    provider_id: &str,
    model_id: &str,
    table: &table::PricingTable,
    overrides: &HashMap<(String, String), TieredPricing>,
) -> Option<TieredPricing> {
    if let Some(tp) = override_lookup(provider_id, model_id, overrides) {
        return Some(tp.clone());
    }
    table
        .lookup(model_id)
        .map(|off_peak| TieredPricing { off_peak, peak: None })
}

/// 校验四个单价非负（允许 0，禁止为负）。返回中文错误信息。
pub fn validate_non_negative(p: &ModelPricing) -> Result<(), String> {
    let fields = [
        ("输入", p.input),
        ("输出", p.output),
        ("缓存读取", p.cache_read),
        ("缓存创建", p.cache_creation),
    ];
    for (name, v) in fields {
        if v < Decimal::ZERO {
            return Err(format!("{name}单价不能为负：{v}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing::table::PricingTable;
    use std::str::FromStr;

    fn price(i: &str) -> ModelPricing {
        ModelPricing {
            input: Decimal::from_str(i).unwrap(),
            output: Decimal::from_str("1").unwrap(),
            cache_read: Decimal::ZERO,
            cache_creation: Decimal::ZERO,
        }
    }

    /// 未启用峰谷的平价覆盖（既有行为的测试载体）。
    fn tiered(i: &str) -> TieredPricing {
        TieredPricing { off_peak: price(i), peak: None }
    }

    #[test]
    fn override_takes_priority_over_table_for_same_combo() {
        let table = PricingTable::from_rows(vec![("m1".into(), price("0.3"))]);
        let mut overrides = HashMap::new();
        overrides.insert(("p1".to_string(), "m1".to_string()), tiered("9"));

        // 命中覆盖 → 用覆盖价，忽略表价
        assert_eq!(resolve("p1", "m1", &table, &overrides).unwrap().off_peak.input,
                   Decimal::from_str("9").unwrap());
        // 未覆盖的供应商 → 回退表价
        assert_eq!(resolve("p2", "m1", &table, &overrides).unwrap().off_peak.input,
                   Decimal::from_str("0.3").unwrap());
        // 表和覆盖都没有 → None
        assert!(resolve("p1", "nope", &table, &overrides).is_none());
    }

    #[test]
    fn override_lookup_matches_namespace_record_to_bare_override() {
        let mut overrides = HashMap::new();
        overrides.insert(
            ("p1".to_string(), "deepseek-v4.1-flash".to_string()),
            tiered("0.15"),
        );

        // 命名空间记录命中裸名覆盖
        let got = override_lookup("p1", "deepseek-ai/DeepSeek-V4.1-flash", &overrides);
        assert_eq!(got.unwrap().off_peak.input, Decimal::from_str("0.15").unwrap());
        // :high 后缀记录命中
        let got = override_lookup("p1", "deepseek-v4.1-flash:high", &overrides);
        assert!(got.is_some(), ":tag 应被 clean 去掉后命中");
        // 跨 provider 不命中（provider 始终精确匹配）
        assert!(override_lookup("p2", "deepseek-ai/DeepSeek-V4.1-flash", &overrides).is_none(),
                "禁止跨供应商命中");
        // 完全无关的模型不命中
        assert!(override_lookup("p1", "other-model", &overrides).is_none());
    }

    /// 反向：覆盖键带命名空间/标签、记录为裸名，也应命中（同一套归一化规则）。
    #[test]
    fn override_lookup_matches_bare_record_to_namespaced_override() {
        let mut overrides = HashMap::new();
        overrides.insert(
            ("p1".to_string(), "deepseek-ai/DeepSeek-V4.1-flash:high".to_string()),
            tiered("0.2"),
        );
        let got = override_lookup("p1", "deepseek-v4.1-flash", &overrides);
        assert_eq!(got.unwrap().off_peak.input, Decimal::from_str("0.2").unwrap());
    }

    /// 精确命中优先于归一化命中。
    #[test]
    fn override_lookup_exact_beats_normalized() {
        let mut overrides = HashMap::new();
        overrides.insert(("p1".to_string(), "m1".to_string()), tiered("1"));
        overrides.insert(("p1".to_string(), "m1:high".to_string()), tiered("2"));

        // 记录 m1:high：精确命中 2，而不是归一化到 m1 的 1
        let got = override_lookup("p1", "m1:high", &overrides);
        assert_eq!(got.unwrap().off_peak.input, Decimal::from_str("2").unwrap());
        // 记录 m1：精确命中 1
        let got = override_lookup("p1", "m1", &overrides);
        assert_eq!(got.unwrap().off_peak.input, Decimal::from_str("1").unwrap());
    }

    /// 覆盖键归一化后为空（unknown 类）→ 不进归一化索引，不误伤其它模型；
    /// 与记录精确同键的覆盖仍按精确匹配生效（优先级最高）。
    #[test]
    fn override_lookup_skips_empty_candidate_overrides() {
        let mut overrides = HashMap::new();
        overrides.insert(("p1".to_string(), "unknown".to_string()), tiered("9"));
        assert!(override_lookup("p1", "anything", &overrides).is_none(),
                "空候选覆盖不得命中其它模型");
        // (p1, unknown) 精确键对同键记录照常生效（精确优先，与归一化无关）
        let got = override_lookup("p1", "unknown", &overrides);
        assert_eq!(got.unwrap().off_peak.input, Decimal::from_str("9").unwrap(),
                   "精确键命中不受候选为空影响");

        // 与正常覆盖共存时不干扰
        overrides.insert(("p1".to_string(), "real".to_string()), tiered("3"));
        let got = override_lookup("p1", "real", &overrides);
        assert_eq!(got.unwrap().off_peak.input, Decimal::from_str("3").unwrap());
    }

    /// 归一化命中的覆盖同样优先于表价（口径统一后 resolve 的行为扩展）。
    #[test]
    fn resolve_prefers_normalized_override_over_table() {
        let table = PricingTable::from_rows(vec![("m1".into(), price("0.3"))]);
        let mut overrides = HashMap::new();
        // 覆盖键大小写不同（归一化到 m1），价 0.9
        overrides.insert(("p1".to_string(), "M1".to_string()), tiered("0.9"));

        // p1: 归一化命中覆盖 → 覆盖价（而非表价 0.3）
        assert_eq!(resolve("p1", "m1", &table, &overrides).unwrap().off_peak.input,
                   Decimal::from_str("0.9").unwrap());
        // 命名空间记录同样命中
        assert_eq!(resolve("p1", "ns/m1", &table, &overrides).unwrap().off_peak.input,
                   Decimal::from_str("0.9").unwrap());
        // p2 无覆盖 → 回退表价
        assert_eq!(resolve("p2", "m1", &table, &overrides).unwrap().off_peak.input,
                   Decimal::from_str("0.3").unwrap());
    }

    /// 表价命中 → peak = None（cc-switch 表价无峰谷概念）。
    #[test]
    fn resolve_from_table_yields_no_peak() {
        let table = PricingTable::from_rows(vec![("m1".into(), price("0.3"))]);
        let tp = resolve("p1", "m1", &table, &HashMap::new()).unwrap();
        assert_eq!(tp.off_peak.input, Decimal::from_str("0.3").unwrap());
        assert_eq!(tp.peak, None);
    }

    /// 覆盖命中 → 带出该覆盖的高峰价。
    #[test]
    fn resolve_from_override_carries_peak() {
        let table = PricingTable::from_rows(vec![]);
        let mut overrides = HashMap::new();
        overrides.insert(
            ("p1".to_string(), "m1".to_string()),
            TieredPricing { off_peak: price("1"), peak: Some(price("2")) },
        );
        let tp = resolve("p1", "m1", &table, &overrides).unwrap();
        assert_eq!(tp.peak.map(|p| p.input), Some(Decimal::from_str("2").unwrap()));
    }

    /// pick：峰段选 peak、谷段选 off_peak；peak 未配置时任何时刻都回落 off_peak。
    #[test]
    fn tiered_pick_selects_by_started_at_and_falls_back() {
        // 2026-10-12 为周一（节假日表外的普通工作日），days_from_civil = 20738
        const DAY: i64 = 86_400_000;
        let t_peak = 20_738 * DAY + 2 * 3_600_000;  // 北京 10:00 = UTC 02:00
        let t_off = 20_738 * DAY + 11 * 3_600_000;  // 北京 19:00 = UTC 11:00
        assert!(tier::is_peak(t_peak) && !tier::is_peak(t_off), "时间戳自检");

        let off = price("1");
        let peak = price("2");
        let tp = TieredPricing { off_peak: off.clone(), peak: Some(peak.clone()) };
        assert_eq!(tp.pick(t_peak), &peak, "峰段应选高峰价");
        assert_eq!(tp.pick(t_off), &off, "谷段应选空闲价");

        // peak 未配置（未启用峰谷 / 表价）→ 恒回落空闲档
        let flat = TieredPricing { off_peak: off.clone(), peak: None };
        assert_eq!(flat.pick(t_peak), &off);
        assert_eq!(flat.pick(t_off), &off);
        assert_eq!(flat.pick(0), &off);
    }

    #[test]
    fn validate_rejects_negative_but_allows_zero() {
        let zero = ModelPricing {
            input: Decimal::ZERO, output: Decimal::ZERO,
            cache_read: Decimal::ZERO, cache_creation: Decimal::ZERO,
        };
        assert!(validate_non_negative(&zero).is_ok(), "全 0 必须允许");

        for (name, p) in [
            ("输入", ModelPricing { input: Decimal::from_str("-0.1").unwrap(), ..zero.clone() }),
            ("输出", ModelPricing { output: Decimal::from_str("-1").unwrap(), ..zero.clone() }),
            ("缓存读取", ModelPricing { cache_read: Decimal::from_str("-2").unwrap(), ..zero.clone() }),
            ("缓存创建", ModelPricing { cache_creation: Decimal::from_str("-3").unwrap(), ..zero.clone() }),
        ] {
            let err = validate_non_negative(&p).unwrap_err();
            assert!(err.contains(name), "错误信息应指出字段 {name}: {err}");
            assert!(err.contains("不能为负"), "错误信息应为中文负价提示: {err}");
        }
    }
}
