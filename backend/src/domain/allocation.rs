use rust_decimal::Decimal;

/// Synthetic `category_id` for the "Lainnya" bucket — assets that don't map to
/// any target category. Negative so it can never collide with a real DB id.
pub const UNCATEGORIZED_CATEGORY_ID: i64 = -1;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CategoryAllocation {
    pub category_id: i64,
    pub name: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub target_pct: Decimal,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub tolerance_band_pct: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str")]
    pub actual_pct: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub actual_value_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub drift_pct: Decimal,
    pub out_of_band: bool,
    #[serde(with = "rust_decimal::serde::str")]
    pub rebalance_idr: Decimal,
}

pub struct CategoryInput {
    pub category_id: i64,
    pub name: String,
    pub target_pct: Decimal,
    pub tolerance_band_pct: Option<Decimal>,
    pub value_idr: Decimal,
}

pub fn compute_allocation(cats: &[CategoryInput]) -> Vec<CategoryAllocation> {
    let total: Decimal = cats.iter().map(|c| c.value_idr).sum();
    let hundred = Decimal::from(100);
    cats.iter()
        .map(|c| {
            let actual_pct = if total.is_zero() {
                Decimal::ZERO
            } else {
                c.value_idr / total * hundred
            };
            let drift = actual_pct - c.target_pct;
            let out_of_band = match c.tolerance_band_pct {
                Some(band) => drift.abs() > band,
                None => false,
            };
            let target_value = total * c.target_pct / hundred;
            CategoryAllocation {
                category_id: c.category_id,
                name: c.name.clone(),
                target_pct: c.target_pct,
                tolerance_band_pct: c.tolerance_band_pct,
                actual_pct,
                actual_value_idr: c.value_idr,
                drift_pct: drift,
                out_of_band,
                rebalance_idr: target_value - c.value_idr,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn computes_drift_and_out_of_band() {
        let cats = vec![
            CategoryInput {
                category_id: 1,
                name: "USD ETF".into(),
                target_pct: dec!(50),
                tolerance_band_pct: Some(dec!(5)),
                value_idr: dec!(400),
            },
            CategoryInput {
                category_id: 2,
                name: "Saham ID".into(),
                target_pct: dec!(50),
                tolerance_band_pct: Some(dec!(5)),
                value_idr: dec!(600),
            },
        ];
        let out = compute_allocation(&cats);
        let c1 = &out[0];
        assert_eq!(c1.actual_pct, dec!(40));
        assert_eq!(c1.drift_pct, dec!(-10));
        assert!(c1.out_of_band);
        assert_eq!(c1.rebalance_idr, dec!(100));
    }

    #[test]
    fn empty_portfolio_is_zero_not_panic() {
        let cats = vec![CategoryInput {
            category_id: 1,
            name: "X".into(),
            target_pct: dec!(100),
            tolerance_band_pct: None,
            value_idr: dec!(0),
        }];
        let out = compute_allocation(&cats);
        assert_eq!(out[0].actual_pct, dec!(0));
    }
}
