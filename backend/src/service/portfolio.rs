use crate::db::Db;
use crate::domain::allocation::{
    compute_allocation, CategoryAllocation, CategoryInput, UNCATEGORIZED_CATEGORY_ID,
};
use crate::domain::cost_basis::compute_cost_basis;
use crate::domain::models::TxnType;
use crate::domain::plan_alloc::{compute_plan_tree, PlanNodeAllocation, PlanNodeInput};
use crate::domain::valuation::{build_position, group_by_instrument, PriceContext, Position};
use crate::domain::xirr::{xirr, CashFlow};
use crate::repo::{categories, dec, instruments, plan_nodes, prices, transactions};
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
    #[serde(with = "rust_decimal::serde::str")]
    pub total_unrealized_price_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub total_unrealized_fx_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub total_realized_price_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub total_realized_fx_pnl_idr: Decimal,
    /// True when any position's IDR decomposition is based on incomplete FX data
    /// (see resolve_fx_gaps) — aggregate totals may understate IDR cost basis.
    pub fx_incomplete: bool,
    pub xirr: Option<f64>,
    pub positions: Vec<Position>,
    pub allocation: Vec<CategoryAllocation>,
}

/// Backfill zero `fx_to_idr` on historical txns from the fx_rate table (rate
/// at-or-before the txn date). Returns true when at least one txn still has no
/// usable rate — callers must surface this (Position.fx_incomplete), never
/// silently substitute the current rate.
async fn resolve_fx_gaps(db: &Db, txns: &mut [crate::domain::models::Transaction]) -> anyhow::Result<bool> {
    let mut incomplete = false;
    for t in txns.iter_mut() {
        if !t.fx_to_idr.is_zero() {
            continue;
        }
        if t.currency == "IDR" {
            t.fx_to_idr = Decimal::ONE;
            continue;
        }
        let date = t.executed_at.format("%Y-%m-%d").to_string();
        match prices::fx_on(db, &t.currency, "IDR", &date).await? {
            Some(rate) if !rate.is_zero() => t.fx_to_idr = rate,
            _ => incomplete = true,
        }
    }
    Ok(incomplete)
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
    let mut unreal_price_idr = Decimal::ZERO;
    let mut unreal_fx_idr = Decimal::ZERO;
    let mut real_price_idr = Decimal::ZERO;
    let mut real_fx_idr = Decimal::ZERO;

    for (instrument_id, mut txns) in grouped {
        let fx_incomplete = resolve_fx_gaps(db, &mut txns).await?;
        let cb = compute_cost_basis(&txns);
        let ins = instruments::get(db, instrument_id).await?;
        let latest = prices::latest(db, instrument_id).await?;
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
        let ctx = PriceContext { instrument_id, latest_price_native: price, price_stale: stale, fx_native_to_idr: to_idr, fx_native_to_usd: to_usd };
        let p = build_position(instrument_id, &cb, &ctx, fx_incomplete);
        net_idr += p.market_value_idr;
        net_usd += p.market_value_usd;
        // FX-aware totals: cost basis at purchase-time rates, value at today's rate.
        unreal_idr += p.unrealized_pnl_idr;
        real_idr += p.realized_pnl_idr;
        unreal_price_idr += p.unrealized_price_pnl_idr;
        unreal_fx_idr += p.unrealized_fx_pnl_idr;
        real_price_idr += p.realized_price_pnl_idr;
        real_fx_idr += p.realized_fx_pnl_idr;
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
    // Surface assets that don't map to any target category (no category, or a
    // category that no longer exists) as a synthetic "Lainnya" bucket. This makes
    // the allocation total reconcile with net worth and rebases every percentage
    // against the whole portfolio instead of just the categorized slice. The
    // bucket carries no target, so it never flags out-of-band.
    let categorized_total: Decimal = cat_inputs.iter().map(|c| c.value_idr).sum();
    let uncategorized_idr = net_idr - categorized_total;
    if uncategorized_idr > Decimal::ZERO {
        cat_inputs.push(CategoryInput {
            category_id: UNCATEGORIZED_CATEGORY_ID,
            name: "Lainnya".to_string(),
            target_pct: Decimal::ZERO,
            tolerance_band_pct: None,
            value_idr: uncategorized_idr,
        });
    }
    let allocation = compute_allocation(&cat_inputs);

    let fx_incomplete = positions.iter().any(|p| p.fx_incomplete);

    // XIRR from cashflows: buys/deposits negative, sells/dividends positive, plus current net worth as terminal inflow.
    // Intentionally reads the original txns (not the gap-filled per-instrument clones):
    // XIRR is valued via fx_to_usd, which resolve_fx_gaps does not touch.
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
        total_unrealized_price_pnl_idr: unreal_price_idr,
        total_unrealized_fx_pnl_idr: unreal_fx_idr,
        total_realized_price_pnl_idr: real_price_idr,
        total_realized_fx_pnl_idr: real_fx_idr,
        fx_incomplete,
        xirr: xirr_val,
        positions,
        allocation,
    })
}

/// Build the recursive allocation tree (plan_node overlay). Reuses build_summary's
/// per-instrument market values so valuation stays single-sourced.
pub async fn build_plan_tree(db: &Db) -> anyhow::Result<Vec<PlanNodeAllocation>> {
    let summary = build_summary(db).await?;
    let total = summary.net_worth_idr;

    let mut instrument_value = std::collections::HashMap::new();
    for p in &summary.positions {
        instrument_value.insert(p.instrument_id, p.market_value_idr);
    }
    let mut instrument_category = std::collections::HashMap::new();
    for ins in instruments::list(db).await? {
        instrument_category.insert(ins.id, ins.category_id);
    }

    let mut inputs = Vec::new();
    for r in plan_nodes::list(db).await? {
        inputs.push(PlanNodeInput {
            id: r.id,
            parent_id: r.parent_id,
            name: r.name,
            target_pct: dec(&r.target_pct)?,
            tolerance_band_pct: r.tolerance_band_pct.as_deref().map(dec).transpose()?,
            bind_kind: r.bind_kind,
            category_id: r.category_id,
            instrument_id: r.instrument_id,
            sort_order: r.sort_order,
            color: r.color,
        });
    }

    Ok(compute_plan_tree(&inputs, &instrument_value, &instrument_category, total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::TxnType;
    use crate::repo::dec;
    use chrono::Utc;

    #[tokio::test]
    async fn plan_tree_breaks_category_into_instrument_and_lainnya() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acct = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount{ name:"X".into(), account_type:"manual".into(), institution:None, native_currency:"IDR".into(), note:None }).await.unwrap();
        let cat = categories::create(&db, &categories::NewCategory { name:"Saham IDX".into(), target_pct:"100".into(), tolerance_band_pct:Some("5".into()), sort_order:None, color:None }).await.unwrap();
        // Migration backfill only covers categories that existed at connect() time; this
        // category was created afterward, so create its root plan_node explicitly (this is
        // what the frontend "add allocation" flow will do for new categories).
        let root = crate::repo::plan_nodes::create(&db, &crate::repo::plan_nodes::NewPlanNode{
            parent_id: None, name: "Saham".into(), target_pct: "100".into(),
            tolerance_band_pct: Some("5".into()), bind_kind: "category".into(),
            category_id: Some(cat.id), instrument_id: None, sort_order: None, color: None,
        }).await.unwrap();

        // Two stocks in the category, each worth 100 IDR.
        let bbca = instruments::create(&db, &instruments::NewInstrument{ symbol:"BBCA".into(), name:"BBCA".into(), instrument_type:"stock".into(), native_currency:"IDR".into(), category_id:Some(cat.id), price_source:"manual".into(), decimals:Some(0), note:None }).await.unwrap();
        let bbri = instruments::create(&db, &instruments::NewInstrument{ symbol:"BBRI".into(), name:"BBRI".into(), instrument_type:"stock".into(), native_currency:"IDR".into(), category_id:Some(cat.id), price_source:"manual".into(), decimals:Some(0), note:None }).await.unwrap();
        for ins in [bbca.id, bbri.id] {
            transactions::create(&db, &transactions::NewTransaction{ account_id:acct.id, instrument_id:ins, txn_type:"buy".into(), executed_at:chrono::Utc::now(), quantity:"1".into(), price_native:"100".into(), fee_native:None, currency:"IDR".into(), fx_to_idr:"1".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None }).await.unwrap();
            prices::upsert_latest(&db, ins, dec("100").unwrap(), "IDR", "test", "2099-01-01").await.unwrap();
        }
        // Break out BBCA as an instrument child of the category root.
        crate::repo::plan_nodes::create(&db, &crate::repo::plan_nodes::NewPlanNode{
            parent_id: Some(root.id), name: "BBCA".into(), target_pct: "50".into(),
            tolerance_band_pct: None, bind_kind: "instrument".into(),
            category_id: None, instrument_id: Some(bbca.id), sort_order: None, color: None,
        }).await.unwrap();

        let tree = build_plan_tree(&db).await.unwrap();
        let saham = tree.iter().find(|n| n.id == root.id).unwrap();
        assert_eq!(saham.actual_value_idr, dec("200").unwrap()); // both stocks
        let bbca_node = saham.children.iter().find(|n| n.name == "BBCA").unwrap();
        assert_eq!(bbca_node.actual_value_idr, dec("100").unwrap());
        let lain = saham.children.iter().find(|n| n.name == "Lainnya").unwrap();
        assert_eq!(lain.actual_value_idr, dec("100").unwrap()); // BBRI remainder
    }

    #[tokio::test]
    async fn resolve_fx_gaps_backfills_from_fx_rate_table() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        prices::upsert_fx(&db, "USD", "IDR", dec("15500").unwrap(), "2026-01-01").await.unwrap();
        let mut txns = vec![crate::domain::models::Transaction {
            id: 1, account_id: 1, instrument_id: 1,
            txn_type: TxnType::Buy,
            executed_at: chrono::DateTime::parse_from_rfc3339("2026-02-01T00:00:00Z").unwrap().with_timezone(&Utc),
            quantity: dec("1").unwrap(), price_native: dec("100").unwrap(),
            fee_native: dec("0").unwrap(), currency: "USD".into(),
            fx_to_idr: dec("0").unwrap(), fx_to_usd: dec("1").unwrap(), note: None,
        }];
        let incomplete = resolve_fx_gaps(&db, &mut txns).await.unwrap();
        assert!(!incomplete);
        assert_eq!(txns[0].fx_to_idr, dec("15500").unwrap());
    }

    #[tokio::test]
    async fn resolve_fx_gaps_flags_unresolvable_txn() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        // No fx_rate rows at all -> can't backfill, must flag.
        let mut txns = vec![crate::domain::models::Transaction {
            id: 1, account_id: 1, instrument_id: 1,
            txn_type: TxnType::Buy,
            executed_at: chrono::DateTime::parse_from_rfc3339("2026-02-01T00:00:00Z").unwrap().with_timezone(&Utc),
            quantity: dec("1").unwrap(), price_native: dec("100").unwrap(),
            fee_native: dec("0").unwrap(), currency: "USD".into(),
            fx_to_idr: dec("0").unwrap(), fx_to_usd: dec("1").unwrap(), note: None,
        }];
        let incomplete = resolve_fx_gaps(&db, &mut txns).await.unwrap();
        assert!(incomplete);
        assert_eq!(txns[0].fx_to_idr, dec("0").unwrap()); // untouched, not guessed
    }

    #[tokio::test]
    async fn summary_captures_fx_gain_on_usd_position() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let cat = categories::create(&db, &categories::NewCategory { name:"Crypto".into(), target_pct:"100".into(), tolerance_band_pct:Some("5".into()), sort_order:None, color:None }).await.unwrap();
        let acct = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount{ name:"X".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument{ symbol:"BTC".into(), name:"BTC".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:Some(cat.id), price_source:"coingecko:bitcoin".into(), decimals:Some(8), note:None }).await.unwrap();
        // Bought at fx 16000; IDR has since weakened to 17000.
        transactions::create(&db, &transactions::NewTransaction{ account_id:acct.id, instrument_id:ins.id, txn_type:"buy".into(), executed_at:Utc::now(), quantity:"1".into(), price_native:"100".into(), fee_native:None, currency:"USD".into(), fx_to_idr:"16000".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None }).await.unwrap();
        prices::upsert_latest(&db, ins.id, dec("150").unwrap(), "USD", "test", "2099-01-01").await.unwrap();
        prices::upsert_fx(&db, "USD", "IDR", dec("17000").unwrap(), "2099-01-01").await.unwrap();

        let s = build_summary(&db).await.unwrap();
        // total = 150*17000 - 100*16000 = 950,000 ; price = 50*17000 = 850,000 ; fx = 100,000
        assert_eq!(s.total_unrealized_pnl_idr, dec("950000").unwrap());
        assert_eq!(s.total_unrealized_price_pnl_idr, dec("850000").unwrap());
        assert_eq!(s.total_unrealized_fx_pnl_idr, dec("100000").unwrap());
        // Aggregate invariant: decomposition sums to the total exactly.
        assert_eq!(
            s.total_unrealized_pnl_idr,
            s.total_unrealized_price_pnl_idr + s.total_unrealized_fx_pnl_idr
        );
        let p = &s.positions[0];
        assert!(!p.fx_incomplete);
        assert_eq!(p.unrealized_fx_pnl_idr, dec("100000").unwrap());
    }

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

    #[tokio::test]
    async fn summary_groups_uncategorized_into_lainnya_bucket() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acct = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount{ name:"X".into(), account_type:"manual".into(), institution:None, native_currency:"IDR".into(), note:None }).await.unwrap();
        let cat = categories::create(&db, &categories::NewCategory { name:"Saham IDX".into(), target_pct:"100".into(), tolerance_band_pct:Some("5".into()), sort_order:None, color:None }).await.unwrap();

        // Categorized position worth 100 IDR.
        let ins_a = instruments::create(&db, &instruments::NewInstrument{ symbol:"AAA".into(), name:"AAA".into(), instrument_type:"stock".into(), native_currency:"IDR".into(), category_id:Some(cat.id), price_source:"manual".into(), decimals:Some(0), note:None }).await.unwrap();
        transactions::create(&db, &transactions::NewTransaction{ account_id:acct.id, instrument_id:ins_a.id, txn_type:"buy".into(), executed_at:Utc::now(), quantity:"1".into(), price_native:"100".into(), fee_native:None, currency:"IDR".into(), fx_to_idr:"1".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None }).await.unwrap();
        prices::upsert_latest(&db, ins_a.id, dec("100").unwrap(), "IDR", "test", "2099-01-01").await.unwrap();

        // Uncategorized position (category_id = None) also worth 100 IDR.
        let ins_b = instruments::create(&db, &instruments::NewInstrument{ symbol:"BBB".into(), name:"BBB".into(), instrument_type:"stock".into(), native_currency:"IDR".into(), category_id:None, price_source:"manual".into(), decimals:Some(0), note:None }).await.unwrap();
        transactions::create(&db, &transactions::NewTransaction{ account_id:acct.id, instrument_id:ins_b.id, txn_type:"buy".into(), executed_at:Utc::now(), quantity:"1".into(), price_native:"100".into(), fee_native:None, currency:"IDR".into(), fx_to_idr:"1".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None }).await.unwrap();
        prices::upsert_latest(&db, ins_b.id, dec("100").unwrap(), "IDR", "test", "2099-01-01").await.unwrap();

        let s = build_summary(&db).await.unwrap();
        assert_eq!(s.net_worth_idr, dec("200").unwrap());

        let lainnya = s.allocation.iter().find(|c| c.category_id == UNCATEGORIZED_CATEGORY_ID).expect("Lainnya bucket present");
        assert_eq!(lainnya.name, "Lainnya");
        assert_eq!(lainnya.actual_value_idr, dec("100").unwrap());
        assert_eq!(lainnya.actual_pct, dec("50").unwrap());
        assert_eq!(lainnya.target_pct, dec("0").unwrap());
        assert!(!lainnya.out_of_band, "Lainnya must never flag out-of-band");

        // Percentages rebase against net worth: the categorized slice is now 50%,
        // and the whole allocation reconciles to net worth.
        let saham = s.allocation.iter().find(|c| c.category_id == cat.id).unwrap();
        assert_eq!(saham.actual_pct, dec("50").unwrap());
        let total: Decimal = s.allocation.iter().map(|c| c.actual_value_idr).sum();
        assert_eq!(total, s.net_worth_idr);
    }

    #[tokio::test]
    async fn summary_flags_fx_incomplete_when_rate_unresolvable() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let cat = categories::create(&db, &categories::NewCategory { name:"Crypto".into(), target_pct:"100".into(), tolerance_band_pct:Some("5".into()), sort_order:None, color:None }).await.unwrap();
        let acct = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount{ name:"X".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument{ symbol:"BTC".into(), name:"BTC".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:Some(cat.id), price_source:"coingecko:bitcoin".into(), decimals:Some(8), note:None }).await.unwrap();
        // fx_to_idr 0 and NO fx_rate row for the txn date -> unresolvable.
        transactions::create(&db, &transactions::NewTransaction{ account_id:acct.id, instrument_id:ins.id, txn_type:"buy".into(), executed_at:Utc::now(), quantity:"1".into(), price_native:"100".into(), fee_native:None, currency:"USD".into(), fx_to_idr:"0".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None }).await.unwrap();
        prices::upsert_latest(&db, ins.id, dec("150").unwrap(), "USD", "test", "2099-01-01").await.unwrap();

        let s = build_summary(&db).await.unwrap();
        assert!(s.fx_incomplete);
        assert!(s.positions[0].fx_incomplete);
    }
}
