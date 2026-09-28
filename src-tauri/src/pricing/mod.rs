pub mod candidates;
pub mod cost;
pub mod table;

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
pub fn override_lookup(
    provider_id: &str,
    model_id: &str,
    overrides: &HashMap<(String, String), ModelPricing>,
) -> Option<ModelPricing> {
    // ① 精确优先。
    if let Some(p) = overrides.get(&(provider_id.to_string(), model_id.to_string())) {
        return Some(p.clone());
    }
    // ② 对该 provider 下的覆盖键建「归一化候选 → 价格」索引。覆盖项按覆盖
    //    model_id 排序后再入索引，多个覆盖键归一化到同一候选时行为确定
    //    （字典序最小者生效）。
    let mut scoped: Vec<(&str, &ModelPricing)> = overrides
        .iter()
        .filter(|((p, _), _)| p == provider_id)
        .map(|((_, m), v)| (m.as_str(), v))
        .collect();
    scoped.sort_by(|a, b| a.0.cmp(b.0));
    let mut index: HashMap<String, &ModelPricing> = HashMap::new();
    for (om, p) in &scoped {
        for c in model_candidates(om) {
            index.entry(c).or_insert(*p);
        }
    }
    // ③ 按记录 model_id 的候选顺序逐个查（原始归一化形态优先于去后缀形态）。
    for c in model_candidates(model_id) {
        if let Some(p) = index.get(&c) {
            return Some((*p).clone());
        }
    }
    None
}

/// 解析单价：`(provider_id, model_id)` 的覆盖优先于 cc-switch 定价表。
pub fn resolve(
    provider_id: &str,
    model_id: &str,
    table: &table::PricingTable,
    overrides: &HashMap<(String, String), ModelPricing>,
) -> Option<ModelPricing> {
    if let Some(p) = override_lookup(provider_id, model_id, overrides) {
        return Some(p);
    }
    table.lookup(model_id)
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

    #[test]
    fn override_takes_priority_over_table_for_same_combo() {
        let table = PricingTable::from_rows(vec![("m1".into(), price("0.3"))]);
        let mut overrides = HashMap::new();
        overrides.insert(("p1".to_string(), "m1".to_string()), price("9"));

        // 命中覆盖 → 用覆盖价，忽略表价
        assert_eq!(resolve("p1", "m1", &table, &overrides).unwrap().input,
                   Decimal::from_str("9").unwrap());
        // 未覆盖的供应商 → 回退表价
        assert_eq!(resolve("p2", "m1", &table, &overrides).unwrap().input,
                   Decimal::from_str("0.3").unwrap());
        // 表和覆盖都没有 → None
        assert!(resolve("p1", "nope", &table, &overrides).is_none());
    }

    #[test]
    fn override_lookup_matches_namespace_record_to_bare_override() {
        let mut overrides = HashMap::new();
        overrides.insert(("p1".to_string(), "deepseek-v4.1-flash".to_string()), price("0.15"));

        // 命名空间记录命中裸名覆盖
        let got = override_lookup("p1", "deepseek-ai/DeepSeek-V4.1-flash", &overrides);
        assert_eq!(got.unwrap().input, Decimal::from_str("0.15").unwrap());
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
            price("0.2"),
        );
        let got = override_lookup("p1", "deepseek-v4.1-flash", &overrides);
        assert_eq!(got.unwrap().input, Decimal::from_str("0.2").unwrap());
    }

    /// 精确命中优先于归一化命中。
    #[test]
    fn override_lookup_exact_beats_normalized() {
        let mut overrides = HashMap::new();
        overrides.insert(("p1".to_string(), "m1".to_string()), price("1"));
        overrides.insert(("p1".to_string(), "m1:high".to_string()), price("2"));

        // 记录 m1:high：精确命中 2，而不是归一化到 m1 的 1
        let got = override_lookup("p1", "m1:high", &overrides);
        assert_eq!(got.unwrap().input, Decimal::from_str("2").unwrap());
        // 记录 m1：精确命中 1
        let got = override_lookup("p1", "m1", &overrides);
        assert_eq!(got.unwrap().input, Decimal::from_str("1").unwrap());
    }

    /// 覆盖键归一化后为空（unknown 类）→ 不进归一化索引，不误伤其它模型；
    /// 与记录精确同键的覆盖仍按精确匹配生效（优先级最高）。
    #[test]
    fn override_lookup_skips_empty_candidate_overrides() {
        let mut overrides = HashMap::new();
        overrides.insert(("p1".to_string(), "unknown".to_string()), price("9"));
        assert!(override_lookup("p1", "anything", &overrides).is_none(),
                "空候选覆盖不得命中其它模型");
        // (p1, unknown) 精确键对同键记录照常生效（精确优先，与归一化无关）
        let got = override_lookup("p1", "unknown", &overrides);
        assert_eq!(got.unwrap().input, Decimal::from_str("9").unwrap(), "精确键命中不受候选为空影响");

        // 与正常覆盖共存时不干扰
        overrides.insert(("p1".to_string(), "real".to_string()), price("3"));
        let got = override_lookup("p1", "real", &overrides);
        assert_eq!(got.unwrap().input, Decimal::from_str("3").unwrap());
    }

    /// 归一化命中的覆盖同样优先于表价（口径统一后 resolve 的行为扩展）。
    #[test]
    fn resolve_prefers_normalized_override_over_table() {
        let table = PricingTable::from_rows(vec![("m1".into(), price("0.3"))]);
        let mut overrides = HashMap::new();
        // 覆盖键大小写不同（归一化到 m1），价 0.9
        overrides.insert(("p1".to_string(), "M1".to_string()), price("0.9"));

        // p1: 归一化命中覆盖 → 覆盖价（而非表价 0.3）
        assert_eq!(resolve("p1", "m1", &table, &overrides).unwrap().input,
                   Decimal::from_str("0.9").unwrap());
        // 命名空间记录同样命中
        assert_eq!(resolve("p1", "ns/m1", &table, &overrides).unwrap().input,
                   Decimal::from_str("0.9").unwrap());
        // p2 无覆盖 → 回退表价
        assert_eq!(resolve("p2", "m1", &table, &overrides).unwrap().input,
                   Decimal::from_str("0.3").unwrap());
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
