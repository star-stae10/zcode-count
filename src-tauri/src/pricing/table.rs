use super::candidates::model_candidates;
use super::ModelPricing;
use crate::error::AppError;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

pub struct PricingTable {
    rows: Vec<(String, ModelPricing)>,
}

impl PricingTable {
    pub fn from_rows(mut rows: Vec<(String, ModelPricing)>) -> Self {
        rows.sort_by(|a, b| a.0.len().cmp(&b.0.len()));
        Self { rows }
    }

    pub fn load(path: &Path) -> Result<Self, AppError> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| AppError::Database(format!("打开 cc-switch 库失败: {e}")))?;
        let mut stmt = conn.prepare(
            "SELECT model_id, input_cost_per_million, output_cost_per_million,
                    cache_read_cost_per_million, cache_creation_cost_per_million
             FROM model_pricing",
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
        let mut rows = Vec::new();
        for r in it {
            let (id, i, o, cr, cc) = r?;
            if let Ok(p) = ModelPricing::from_strings(&i, &o, &cr, &cc) {
                rows.push((id.to_ascii_lowercase(), p));
            }
        }
        Ok(Self::from_rows(rows))
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn lookup(&self, model_id: &str) -> Option<ModelPricing> {
        let candidates = model_candidates(model_id);
        for c in &candidates {
            if let Some((_, p)) = self.rows.iter().find(|(k, _)| k == c) {
                return Some(p.clone());
            }
        }
        for c in &candidates {
            let prefix = format!("{c}-");
            if let Some((_, p)) = self.rows.iter().find(|(k, _)| k.starts_with(&prefix)) {
                return Some(p.clone());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    #[test]
    fn exact_then_prefix_match() {
        let mut t = PricingTable::from_rows(vec![
            ("deepseek-v4-flash".into(), ModelPricing {
                input: Decimal::from_str("0.3").unwrap(),
                output: Decimal::from_str("1.2").unwrap(),
                cache_read: Decimal::from_str("0.006").unwrap(),
                cache_creation: Decimal::ZERO,
            }),
        ]);
        assert!(t.lookup("deepseek-v4-flash").is_some());
        // 前缀：deepseek-v4-flash-0731 -> deepseek-v4-flash
        assert!(t.lookup("deepseek-v4-flash-0731").is_some());
        assert!(t.lookup("nope").is_none());
    }
}
