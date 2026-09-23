use rust_decimal::Decimal;

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
