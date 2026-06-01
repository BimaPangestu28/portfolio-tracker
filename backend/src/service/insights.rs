use rust_decimal::Decimal;
use serde::Serialize;

pub fn savings_rate(income: Decimal, expense: Decimal) -> Decimal {
    if income.is_zero() {
        return Decimal::ZERO;
    }
    (income - expense) / income * Decimal::from(100)
}

pub fn yield_pct(dividend_ttm: Decimal, net_worth: Decimal) -> Decimal {
    if net_worth.is_zero() {
        return Decimal::ZERO;
    }
    dividend_ttm / net_worth * Decimal::from(100)
}

pub fn runway_months(liquid: Decimal, monthly_expense: Decimal) -> Decimal {
    if monthly_expense.is_zero() {
        return Decimal::ZERO;
    }
    liquid / monthly_expense
}

pub fn day_delta(latest: Decimal, prev: Decimal) -> (Decimal, Decimal) {
    let abs = latest - prev;
    let pct = if prev.is_zero() {
        Decimal::ZERO
    } else {
        abs / prev * Decimal::from(100)
    };
    (abs, pct)
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Concentration {
    pub symbol: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub pct: Decimal,
}

/// positions: (symbol, market_value_idr). Returns the largest by value.
pub fn concentration(positions: &[(String, Decimal)], net_worth: Decimal) -> Option<Concentration> {
    let top = positions.iter().max_by(|a, b| a.1.cmp(&b.1))?;
    let pct = if net_worth.is_zero() {
        Decimal::ZERO
    } else {
        top.1 / net_worth * Decimal::from(100)
    };
    Some(Concentration {
        symbol: top.0.clone(),
        pct,
    })
}

/// entries: amounts of dividend/interest txns already filtered to trailing window. Sums them.
pub fn dividend_ttm(amounts_idr: &[Decimal]) -> Decimal {
    amounts_idr.iter().copied().sum()
}

/// positions: (category, market_value_idr). Sums those whose category is in `cash_cats`.
pub fn liquid_idr(positions: &[(String, Decimal)], cash_cats: &[&str]) -> Decimal {
    positions
        .iter()
        .filter(|(c, _)| cash_cats.contains(&c.as_str()))
        .map(|(_, v)| *v)
        .sum()
}

// ---- Assembler types ----

#[derive(Debug, Serialize)]
pub struct CompositionPart {
    pub category: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub value_idr: Decimal,
}

#[derive(Debug, Serialize)]
pub struct CompositionPoint {
    pub as_of: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub total_idr: Decimal,
    pub parts: Vec<CompositionPart>,
}

#[derive(Debug, Serialize)]
pub struct Insights {
    #[serde(with = "rust_decimal::serde::str")]
    pub net_worth_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub day_delta_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub day_delta_pct: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub savings_rate: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub dividend_ttm_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub yield_pct: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub liquid_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub runway_months: Decimal,
    pub concentration: Option<Concentration>,
    pub composition: Vec<CompositionPoint>,
    #[serde(with = "rust_decimal::serde::str")]
    pub monthly_income_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub monthly_expense_idr: Decimal,
}

pub async fn build_insights(db: &crate::db::Db) -> anyhow::Result<Insights> {
    use crate::domain::models::TxnType;
    use crate::repo::{cashflow, cashflow_categories, instruments, snapshots, transactions};
    use crate::service::portfolio::build_summary;
    use crate::service::cashflow::{month_summary, CfRow, CatRow};
    use chrono::Utc;

    // 1. Net worth + positions from build_summary
    let summary = build_summary(db).await?;
    let net_worth = summary.net_worth_idr;

    // 2. Build symbol/category maps from instruments
    let instr_list = instruments::list(db).await?;
    // Map instrument_id -> (symbol, instrument_type)
    let ins_map: std::collections::HashMap<i64, (String, String)> = instr_list
        .iter()
        .map(|i| (i.id, (i.symbol.clone(), i.instrument_type.clone())))
        .collect();

    // Build (symbol, mv) and (category, mv) from positions
    // Use instrument_type as the category key; "cash" type = liquid
    let sym_mv: Vec<(String, Decimal)> = summary
        .positions
        .iter()
        .filter_map(|p| {
            ins_map.get(&p.instrument_id).map(|(sym, _)| (sym.clone(), p.market_value_idr))
        })
        .collect();

    let cat_mv: Vec<(String, Decimal)> = summary
        .positions
        .iter()
        .filter_map(|p| {
            ins_map
                .get(&p.instrument_id)
                .map(|(_, itype)| (itype.clone(), p.market_value_idr))
        })
        .collect();

    // 3. Concentration: top holding by market value
    let conc = concentration(&sym_mv, net_worth);

    // 4. Liquidity: sum of positions where instrument_type == "cash"
    let liq = liquid_idr(&cat_mv, &["cash"]);

    // 5. Dividend TTM: sum of dividend/interest txns within last 365 days
    let all_txns = transactions::list_all(db).await?;
    let cutoff = Utc::now() - chrono::Duration::days(365);
    let div_amounts: Vec<Decimal> = all_txns
        .iter()
        .filter(|t| {
            matches!(t.txn_type, TxnType::Dividend | TxnType::Interest)
                && t.executed_at >= cutoff
        })
        .map(|t| t.quantity * t.price_native * t.fx_to_idr)
        .collect();
    let div_ttm = dividend_ttm(&div_amounts);

    // 6. Day delta from last two snapshots
    let snap_history = snapshots::history(db).await?;
    let (d_abs, d_pct) = if snap_history.len() >= 2 {
        let latest = crate::repo::dec(&snap_history[snap_history.len() - 1].total_idr)?;
        let prev = crate::repo::dec(&snap_history[snap_history.len() - 2].total_idr)?;
        day_delta(latest, prev)
    } else {
        (Decimal::ZERO, Decimal::ZERO)
    };

    // 7. Monthly income/expense from cashflow (current month)
    let now_month = Utc::now().format("%Y-%m").to_string();
    let cf_rows_raw = cashflow::list_for_month(db, &now_month).await?;
    let cf_cats_raw = cashflow_categories::list(db).await?;
    let cf_rows: Vec<CfRow> = cf_rows_raw
        .iter()
        .map(|r| {
            let amount = crate::repo::dec(&r.amount).unwrap_or(Decimal::ZERO);
            CfRow {
                direction: r.direction.clone(),
                amount,
                category_id: r.category_id,
            }
        })
        .collect();
    let cf_cats: Vec<CatRow> = cf_cats_raw
        .iter()
        .map(|c| {
            let budget = c.monthly_budget.as_deref().and_then(|b| crate::repo::dec(b).ok());
            CatRow {
                id: c.id,
                name: c.name.clone(),
                kind: c.kind.clone(),
                budget,
            }
        })
        .collect();
    let ms = month_summary(&now_month, &cf_rows, &cf_cats);
    let monthly_income = ms.total_in;
    let monthly_expense = ms.total_out;

    // 8. Savings rate and yield
    let sr = savings_rate(monthly_income, monthly_expense);
    let yp = yield_pct(div_ttm, net_worth);

    // 9. Runway
    let rw = runway_months(liq, monthly_expense);

    // 10. Composition from snapshot breakdown_json
    let composition = build_composition(&snap_history)?;

    Ok(Insights {
        net_worth_idr: net_worth,
        day_delta_idr: d_abs,
        day_delta_pct: d_pct,
        savings_rate: sr,
        dividend_ttm_idr: div_ttm,
        yield_pct: yp,
        liquid_idr: liq,
        runway_months: rw,
        concentration: conc,
        composition,
        monthly_income_idr: monthly_income,
        monthly_expense_idr: monthly_expense,
    })
}

/// Parse breakdown_json from each snapshot into CompositionPoint entries.
/// breakdown_json is a serialized array of objects with `name` and `actual_value_idr` fields.
fn build_composition(
    snaps: &[crate::repo::snapshots::SnapshotRow],
) -> anyhow::Result<Vec<CompositionPoint>> {
    use serde_json::Value;

    let mut points = Vec::new();
    for snap in snaps {
        let total = crate::repo::dec(&snap.total_idr)?;
        let parts = if snap.breakdown_json.is_empty() || snap.breakdown_json == "[]" || snap.breakdown_json == "{}" {
            Vec::new()
        } else {
            let parsed: Value = serde_json::from_str(&snap.breakdown_json)
                .unwrap_or(Value::Array(vec![]));
            match parsed {
                Value::Array(arr) => arr
                    .iter()
                    .filter_map(|item| {
                        let name = item.get("name")?.as_str()?.to_string();
                        let val_str = item.get("actual_value_idr")?.as_str()?;
                        let val = crate::repo::dec(val_str).ok()?;
                        Some(CompositionPart {
                            category: name,
                            value_idr: val,
                        })
                    })
                    .collect(),
                _ => Vec::new(),
            }
        };
        points.push(CompositionPoint {
            as_of: snap.as_of.clone(),
            total_idr: total,
            parts,
        });
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn savings_rate_normal() {
        assert_eq!(savings_rate(dec!(100), dec!(40)), dec!(60));
    }

    #[test]
    fn savings_rate_zero_income() {
        assert_eq!(savings_rate(Decimal::ZERO, dec!(40)), Decimal::ZERO);
    }

    #[test]
    fn yield_pct_normal() {
        assert_eq!(yield_pct(dec!(10), dec!(1000)), dec!(1));
    }

    #[test]
    fn yield_pct_zero_net_worth() {
        assert_eq!(yield_pct(dec!(10), Decimal::ZERO), Decimal::ZERO);
    }

    #[test]
    fn runway_months_normal() {
        assert_eq!(runway_months(dec!(120), dec!(10)), dec!(12));
    }

    #[test]
    fn runway_months_zero_expense() {
        assert_eq!(runway_months(dec!(120), Decimal::ZERO), Decimal::ZERO);
    }

    #[test]
    fn day_delta_positive() {
        let (abs, pct) = day_delta(dec!(110), dec!(100));
        assert_eq!(abs, dec!(10));
        // 10/100 * 100 = 10, not 9.09 — wait, 110-100=10, 10/100*100=10
        // Actually 10/100 * 100 = 10. Let me recheck: day_delta(110,100) -> (10, 10%)
        assert_eq!(pct, dec!(10));
    }

    #[test]
    fn day_delta_approx_pct() {
        // 110 - 100 = 10, 10/100*100 = 10%
        let (abs, pct) = day_delta(dec!(110), dec!(100));
        assert_eq!(abs, dec!(10));
        assert_eq!(pct, dec!(10));
    }

    #[test]
    fn day_delta_zero_prev() {
        let (abs, pct) = day_delta(dec!(110), Decimal::ZERO);
        assert_eq!(abs, dec!(110));
        assert_eq!(pct, Decimal::ZERO);
    }

    #[test]
    fn concentration_picks_max() {
        let positions = vec![
            ("AAAA".to_string(), dec!(300)),
            ("BBBB".to_string(), dec!(700)),
            ("CCCC".to_string(), dec!(100)),
        ];
        let c = concentration(&positions, dec!(1100)).unwrap();
        assert_eq!(c.symbol, "BBBB");
        // 700/1100 * 100 ≈ 63.636...
        let expected = dec!(700) / dec!(1100) * Decimal::from(100);
        assert_eq!(c.pct, expected);
    }

    #[test]
    fn concentration_zero_net_worth() {
        let positions = vec![("X".to_string(), dec!(100))];
        let c = concentration(&positions, Decimal::ZERO).unwrap();
        assert_eq!(c.pct, Decimal::ZERO);
    }

    #[test]
    fn concentration_empty() {
        assert!(concentration(&[], dec!(100)).is_none());
    }

    #[test]
    fn dividend_ttm_sums() {
        let amounts = vec![dec!(100), dec!(200), dec!(50)];
        assert_eq!(dividend_ttm(&amounts), dec!(350));
    }

    #[test]
    fn dividend_ttm_empty() {
        assert_eq!(dividend_ttm(&[]), Decimal::ZERO);
    }

    #[test]
    fn liquid_idr_filters_by_category() {
        let positions = vec![
            ("cash".to_string(), dec!(100)),
            ("crypto".to_string(), dec!(500)),
            ("cash".to_string(), dec!(200)),
            ("saham".to_string(), dec!(300)),
        ];
        assert_eq!(liquid_idr(&positions, &["cash"]), dec!(300));
    }

    #[test]
    fn liquid_idr_no_cash() {
        let positions = vec![
            ("crypto".to_string(), dec!(500)),
            ("saham".to_string(), dec!(300)),
        ];
        assert_eq!(liquid_idr(&positions, &["cash"]), Decimal::ZERO);
    }
}
