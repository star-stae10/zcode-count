pub mod candidates;
pub mod cost;
pub mod table;

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

/// 解析单价：`(provider_id, model_id)` 的覆盖优先于 cc-switch 定价表。
pub fn resolve(
    provider_id: &str,
    model_id: &str,
    table: &table::PricingTable,
    overrides: &HashMap<(String, String), ModelPricing>,
) -> Option<ModelPricing> {
    if let Some(p) = overrides.get(&(provider_id.to_string(), model_id.to_string())) {
        return Some(p.clone());
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
