use rust_decimal::Decimal;
use serde::Serialize;

#[derive(Debug, Serialize, PartialEq)]
pub struct CategoryLine {
    pub category_id: Option<i64>,
    pub name: String,
    pub kind: String,
    #[serde(with = "rust_decimal::serde::str")] pub actual: Decimal,
    #[serde(with = "rust_decimal::serde::str_option")] pub budget: Option<Decimal>,
    pub over_budget: bool,
}
#[derive(Debug, Serialize, PartialEq)]
pub struct MonthSummary {
    pub month: String,
    #[serde(with = "rust_decimal::serde::str")] pub total_in: Decimal,
    #[serde(with = "rust_decimal::serde::str")] pub total_out: Decimal,
    #[serde(with = "rust_decimal::serde::str")] pub net: Decimal,
    pub categories: Vec<CategoryLine>,
}

/// One cashflow row reduced to the fields the aggregator needs.
pub struct CfRow { pub direction: String, pub amount: Decimal, pub category_id: Option<i64> }
/// One category with optional budget.
pub struct CatRow { pub id: i64, pub name: String, pub kind: String, pub budget: Option<Decimal> }

pub fn month_summary(month: &str, rows: &[CfRow], cats: &[CatRow]) -> MonthSummary {
    use std::collections::BTreeMap;
    let mut total_in = Decimal::ZERO;
    let mut total_out = Decimal::ZERO;
    let mut actual: BTreeMap<i64, Decimal> = BTreeMap::new();
    let mut uncat_in = Decimal::ZERO;
    let mut uncat_out = Decimal::ZERO;
    for r in rows {
        if r.direction == "in" { total_in += r.amount; } else { total_out += r.amount; }
        match r.category_id {
            Some(id) => *actual.entry(id).or_insert(Decimal::ZERO) += r.amount,
            None => {
                if r.direction == "in" { uncat_in += r.amount; } else { uncat_out += r.amount; }
            }
        }
    }
    let mut categories = Vec::new();
    for c in cats {
        let act = actual.get(&c.id).copied().unwrap_or(Decimal::ZERO);
        let over = matches!(c.budget, Some(b) if act > b);
        categories.push(CategoryLine { category_id: Some(c.id), name: c.name.clone(), kind: c.kind.clone(), actual: act, budget: c.budget, over_budget: over });
    }
    if uncat_in > Decimal::ZERO {
        categories.push(CategoryLine { category_id: None, name: "Uncategorized income".into(), kind: "income".into(), actual: uncat_in, budget: None, over_budget: false });
    }
    if uncat_out > Decimal::ZERO {
        categories.push(CategoryLine { category_id: None, name: "Uncategorized expense".into(), kind: "expense".into(), actual: uncat_out, budget: None, over_budget: false });
    }
    MonthSummary { month: month.to_string(), total_in, total_out, net: total_in - total_out, categories }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    #[test]
    fn sums_in_out_and_flags_over_budget() {
        let cats = vec![ CatRow { id:1, name:"Food".into(), kind:"expense".into(), budget:Some(dec!(100)) } ];
        let rows = vec![
            CfRow { direction:"in".into(), amount:dec!(500), category_id:None },
            CfRow { direction:"out".into(), amount:dec!(120), category_id:Some(1) },
        ];
        let s = month_summary("2026-06", &rows, &cats);
        assert_eq!(s.total_in, dec!(500));
        assert_eq!(s.total_out, dec!(120));
        assert_eq!(s.net, dec!(380));
        let food = s.categories.iter().find(|c| c.category_id==Some(1)).unwrap();
        assert_eq!(food.actual, dec!(120));
        assert!(food.over_budget);
    }
    #[test]
    fn uncategorized_bucket_has_no_budget() {
        let s = month_summary("2026-06", &[CfRow{direction:"out".into(),amount:dec!(10),category_id:None}], &[]);
        let unc = s.categories.iter().find(|c| c.category_id.is_none()).unwrap();
        assert_eq!(unc.actual, dec!(10));
        assert!(!unc.over_budget);
    }

    #[test]
    fn uncategorized_income_is_labeled_income() {
        let s = month_summary("2026-06", &[CfRow{direction:"in".into(),amount:dec!(500),category_id:None}], &[]);
        let line = s.categories.iter().find(|c| c.category_id.is_none()).unwrap();
        assert_eq!(line.kind, "income");
        assert_eq!(line.actual, dec!(500));
        assert_eq!(s.total_in, dec!(500));
    }
}
