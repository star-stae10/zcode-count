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

pub fn resolve(
    model_id: &str,
    table: &table::PricingTable,
    overrides: &HashMap<String, ModelPricing>,
) -> Option<ModelPricing> {
    if let Some(p) = overrides.get(model_id) {
        return Some(p.clone());
    }
    table.lookup(model_id)
}
