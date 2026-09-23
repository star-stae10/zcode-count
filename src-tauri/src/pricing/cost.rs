use super::ModelPricing;
use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub struct CostBreakdown {
    pub input_cost: Decimal,
    pub output_cost: Decimal,
    pub cache_read_cost: Decimal,
    pub cache_creation_cost: Decimal,
    pub total_cost: Decimal,
}

/// ZCode 的 input_tokens 为 cache-inclusive（总输入），需先扣 cache 再按输入价计费。
pub fn calculate_cache_inclusive(
    input: i64,
    output: i64,
    cache_read: i64,
    cache_creation: i64,
    p: &ModelPricing,
) -> CostBreakdown {
    let million = Decimal::from(1_000_000);
    let billable_input = input
        .saturating_sub(cache_read)
        .saturating_sub(cache_creation)
        .max(0);
    let input_cost = Decimal::from(billable_input) * p.input / million;
    let output_cost = Decimal::from(output) * p.output / million;
    let cache_read_cost = Decimal::from(cache_read) * p.cache_read / million;
    let cache_creation_cost = Decimal::from(cache_creation) * p.cache_creation / million;
    let total_cost = input_cost + output_cost + cache_read_cost + cache_creation_cost;
    CostBreakdown { input_cost, output_cost, cache_read_cost, cache_creation_cost, total_cost }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn p() -> ModelPricing {
        ModelPricing {
            input: Decimal::from_str("0.3").unwrap(),
            output: Decimal::from_str("1.2").unwrap(),
            cache_read: Decimal::from_str("0.006").unwrap(),
            cache_creation: Decimal::ZERO,
        }
    }

    #[test]
    fn cache_inclusive_subtracts_cache_from_input() {
        // input 含 cache：2036 里含 114944? 用合理样例
        let b = calculate_cache_inclusive(53292, 2805, 46464, 0, &p());
        // billable input = 53292 - 46464 = 6828
        assert_eq!(b.input_cost, Decimal::from_str("0.0020484").unwrap());
        assert_eq!(b.output_cost, Decimal::from_str("0.003366").unwrap());
        assert_eq!(b.cache_read_cost, Decimal::from_str("0.000278784").unwrap());
        assert_eq!(b.total_cost, Decimal::from_str("0.005693184").unwrap());
    }

    #[test]
    fn saturating_when_cache_exceeds_input() {
        let b = calculate_cache_inclusive(100, 10, 500, 0, &p());
        assert_eq!(b.input_cost, Decimal::ZERO);
    }
}
