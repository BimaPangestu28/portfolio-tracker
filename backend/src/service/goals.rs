use crate::db::Db;
use crate::domain::goal_progress::{compute_goal_progress, GoalProgress};
use crate::domain::models::TxnType;
use crate::repo::{instruments, prices, transactions};
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Compute a goal's progress (market value / invested / P&L) from its tagged
/// transactions. Per-instrument current price in IDR is the latest quote, or the
/// goal's average buy price for that instrument when no quote exists; native→IDR
/// fx is 1 for IDR instruments and the latest USD/IDR otherwise (mirrors build_summary).
pub async fn build_goal_progress(db: &Db, goal_id: i64) -> anyhow::Result<GoalProgress> {
    let txns = transactions::list_by_goal(db, goal_id).await?;

    // Average buy price per instrument (native), for the no-quote fallback.
    let mut buy_value: HashMap<i64, Decimal> = HashMap::new();
    let mut buy_qty: HashMap<i64, Decimal> = HashMap::new();
    for t in &txns {
        if t.txn_type == TxnType::Buy {
            *buy_value.entry(t.instrument_id).or_default() += t.quantity * t.price_native;
            *buy_qty.entry(t.instrument_id).or_default() += t.quantity;
        }
    }

    let usd_idr = prices::latest_fx(db, "USD", "IDR").await?.unwrap_or(Decimal::ONE);

    // Distinct instruments in the goal.
    let mut instrument_ids: Vec<i64> = txns.iter().map(|t| t.instrument_id).collect();
    instrument_ids.sort_unstable();
    instrument_ids.dedup();

    let mut price_idr: HashMap<i64, Decimal> = HashMap::new();
    for iid in instrument_ids {
        let ins = instruments::get(db, iid).await?;
        let price_native = match prices::latest(db, iid).await? {
            Some(lp) => lp.price,
            None => {
                let qty = buy_qty.get(&iid).copied().unwrap_or(Decimal::ZERO);
                if qty.is_zero() { Decimal::ZERO } else { buy_value.get(&iid).copied().unwrap_or(Decimal::ZERO) / qty }
            }
        };
        let fx = if ins.native_currency == "IDR" { Decimal::ONE } else { usd_idr };
        price_idr.insert(iid, price_native * fx);
    }

    Ok(compute_goal_progress(&txns, &price_idr))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{accounts, goals, instruments, prices, transactions};
    use rust_decimal::Decimal;
    use std::str::FromStr;

    async fn d(s: &str) -> Decimal { Decimal::from_str(s).unwrap() }

    #[tokio::test]
    async fn build_goal_progress_uses_latest_price_for_market_value() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"IDR".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"BBCA".into(), name:"BCA".into(), instrument_type:"stock_id".into(), native_currency:"IDR".into(), category_id:None, price_source:"manual".into(), decimals:Some(0), note:None }).await.unwrap();
        let goal = goals::create(&db, &goals::NewGoal { label:"Pendidikan".into(), note:None, target_idr:"200000000".into(), current_kind:"tagged".into(), current_manual_idr:None, sort_order:None, target_date:None }).await.unwrap();

        let t = transactions::create(&db, &transactions::NewTransaction { account_id:acc.id, instrument_id:ins.id, txn_type:"buy".into(), executed_at:chrono::Utc::now(), quantity:"100".into(), price_native:"9000".into(), fee_native:None, currency:"IDR".into(), fx_to_idr:"1".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None }).await.unwrap();
        transactions::set_txn_goal(&db, t.id, Some(goal.id)).await.unwrap();
        prices::upsert_latest(&db, ins.id, d("9500").await, "IDR", "test", "2099-01-01").await.unwrap();

        let p = build_goal_progress(&db, goal.id).await.unwrap();
        assert_eq!(p.market_value_idr, d("950000").await);  // 100 * 9500
        assert_eq!(p.invested_idr, d("900000").await);      // 100 * 9000
        assert_eq!(p.gain_loss_idr, d("50000").await);
    }

    #[tokio::test]
    async fn build_goal_progress_falls_back_to_avg_buy_when_no_price() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"IDR".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"RDX".into(), name:"Reksadana X".into(), instrument_type:"mutual_fund".into(), native_currency:"IDR".into(), category_id:None, price_source:"manual".into(), decimals:Some(4), note:None }).await.unwrap();
        let goal = goals::create(&db, &goals::NewGoal { label:"G".into(), note:None, target_idr:"1".into(), current_kind:"tagged".into(), current_manual_idr:None, sort_order:None, target_date:None }).await.unwrap();
        let t = transactions::create(&db, &transactions::NewTransaction { account_id:acc.id, instrument_id:ins.id, txn_type:"buy".into(), executed_at:chrono::Utc::now(), quantity:"10".into(), price_native:"1000".into(), fee_native:None, currency:"IDR".into(), fx_to_idr:"1".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None }).await.unwrap();
        transactions::set_txn_goal(&db, t.id, Some(goal.id)).await.unwrap();
        // No price quote -> fall back to avg buy price (1000), so market ≈ invested.
        let p = build_goal_progress(&db, goal.id).await.unwrap();
        assert_eq!(p.market_value_idr, d("10000").await);
        assert_eq!(p.gain_loss_idr, d("0").await);
    }
}
