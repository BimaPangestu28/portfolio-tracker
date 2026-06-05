use crate::db::Db;
use crate::domain::allocation::{compute_allocation, CategoryAllocation, CategoryInput};
use crate::domain::cost_basis::compute_cost_basis;
use crate::domain::models::TxnType;
use crate::domain::valuation::{build_position, group_by_instrument, PriceContext, Position};
use crate::domain::xirr::{xirr, CashFlow};
use crate::repo::{categories, dec, instruments, prices, transactions};
use rust_decimal::Decimal;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct PortfolioSummary {
    #[serde(with = "rust_decimal::serde::str")]
    pub net_worth_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub net_worth_usd: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub total_unrealized_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub total_realized_pnl_idr: Decimal,
    pub xirr: Option<f64>,
    pub positions: Vec<Position>,
    pub allocation: Vec<CategoryAllocation>,
}

pub async fn build_summary(db: &Db) -> anyhow::Result<PortfolioSummary> {
    let usd_idr = prices::latest_fx(db, "USD", "IDR").await?.unwrap_or(Decimal::ONE);
    let all_txns = transactions::list_all(db).await?;
    let grouped = group_by_instrument(all_txns.clone());

    let mut positions = Vec::new();
    let mut net_idr = Decimal::ZERO;
    let mut net_usd = Decimal::ZERO;
    let mut unreal_idr = Decimal::ZERO;
    let mut real_idr = Decimal::ZERO;

    for (instrument_id, txns) in &grouped {
        let cb = compute_cost_basis(txns);
        let ins = instruments::get(db, *instrument_id).await?;
        let latest = prices::latest(db, *instrument_id).await?;
        let (price, stale) = match latest {
            Some(lp) => (lp.price, crate::pricing::service::is_stale(&lp.as_of, chrono::Utc::now(), crate::pricing::service::stale_window_hours(&lp.source))),
            None => (cb.avg_cost, true), // fall back to cost, flagged stale — never silently zero
        };
        // FX from instrument native currency to IDR/USD (Phase 1: non-IDR treated as USD-denominated).
        let (to_idr, to_usd) = if ins.native_currency == "IDR" {
            (Decimal::ONE, if usd_idr.is_zero() { Decimal::ZERO } else { Decimal::ONE / usd_idr })
        } else {
            (usd_idr, Decimal::ONE)
        };
        let ctx = PriceContext { instrument_id: *instrument_id, latest_price_native: price, price_stale: stale, fx_native_to_idr: to_idr, fx_native_to_usd: to_usd };
        let p = build_position(*instrument_id, &cb, &ctx);
        net_idr += p.market_value_idr;
        net_usd += p.market_value_usd;
        unreal_idr += p.unrealized_pnl * to_idr;
        real_idr += p.realized_pnl * to_idr;
        positions.push(p);
    }

    // Allocation by category (value in IDR).
    let cats = categories::list(db).await?;
    let mut ins_cat = std::collections::HashMap::new();
    for ins in instruments::list(db).await? { ins_cat.insert(ins.id, ins.category_id); }

    let mut cat_inputs = Vec::new();
    for c in &cats {
        cat_inputs.push(CategoryInput {
            category_id: c.id,
            name: c.name.clone(),
            target_pct: dec(&c.target_pct)?,
            tolerance_band_pct: c.tolerance_band_pct.as_deref().map(dec).transpose()?,
            value_idr: Decimal::ZERO,
        });
    }
    for p in &positions {
        if let Some(Some(cid)) = ins_cat.get(&p.instrument_id) {
            if let Some(ci) = cat_inputs.iter_mut().find(|c| c.category_id == *cid) {
                ci.value_idr += p.market_value_idr;
            }
        }
    }
    let allocation = compute_allocation(&cat_inputs);

    // XIRR from cashflows: buys/deposits negative, sells/dividends positive, plus current net worth as terminal inflow.
    let mut flows: Vec<CashFlow> = Vec::new();
    for t in &all_txns {
        let amount_usd = ((t.quantity * t.price_native + t.fee_native) * t.fx_to_usd).to_string().parse::<f64>().unwrap_or(0.0);
        let signed = match t.txn_type {
            TxnType::Buy | TxnType::Deposit | TxnType::OpeningBalance => -amount_usd,
            TxnType::Sell | TxnType::Withdrawal | TxnType::Dividend | TxnType::Interest => amount_usd,
            TxnType::Fee => -amount_usd,
        };
        flows.push(CashFlow { date: t.executed_at.date_naive(), amount: signed });
    }
    let terminal = net_usd.to_string().parse::<f64>().unwrap_or(0.0);
    flows.push(CashFlow { date: chrono::Utc::now().date_naive(), amount: terminal });
    let xirr_val = xirr(&flows);

    Ok(PortfolioSummary {
        net_worth_idr: net_idr,
        net_worth_usd: net_usd,
        total_unrealized_pnl_idr: unreal_idr,
        total_realized_pnl_idr: real_idr,
        xirr: xirr_val,
        positions,
        allocation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    #[tokio::test]
    async fn summary_consolidates_one_position() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let cat = categories::create(&db, &categories::NewCategory { name:"Crypto".into(), target_pct:"100".into(), tolerance_band_pct:Some("5".into()), sort_order:None, color:None }).await.unwrap();
        let acct = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount{ name:"X".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument{ symbol:"BTC".into(), name:"BTC".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:Some(cat.id), price_source:"coingecko:bitcoin".into(), decimals:Some(8), note:None }).await.unwrap();
        transactions::create(&db, &transactions::NewTransaction{ account_id:acct.id, instrument_id:ins.id, txn_type:"buy".into(), executed_at:Utc::now(), quantity:"1".into(), price_native:"100".into(), fee_native:None, currency:"USD".into(), fx_to_idr:"16000".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None }).await.unwrap();
        prices::upsert_latest(&db, ins.id, dec("150").unwrap(), "USD", "test", "2099-01-01").await.unwrap();
        prices::upsert_fx(&db, "USD", "IDR", dec("16000").unwrap(), "2099-01-01").await.unwrap();

        let s = build_summary(&db).await.unwrap();
        assert_eq!(s.positions.len(), 1);
        assert_eq!(s.net_worth_usd, dec("150").unwrap());
        assert_eq!(s.net_worth_idr, dec("2400000").unwrap());
        assert_eq!(s.allocation[0].actual_pct, dec("100").unwrap());
    }
}
