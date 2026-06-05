use crate::db::Db;
use serde::Deserialize;

/// The confirm payload the API hands to the service: resolved ids + the (edited) fields.
#[derive(Debug, Deserialize)]
pub struct ConfirmPayload {
    pub account_id: i64,
    pub instrument_id: i64,
    pub entry_type: String,
    pub executed_at: String,        // rfc3339
    pub quantity: String,
    pub price_native: String,
    #[serde(default)]
    pub fee_native: Option<String>,
    pub currency: String,
    #[serde(default)]
    pub fx_to_idr: Option<String>,  // if absent, default from latest USD/IDR
    #[serde(default)]
    pub fx_to_usd: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    /// Total transaction value (e.g. Bibit mutual fund buys show only an IDR
    /// amount). Used when quantity/price are absent: quantity = amount, price = 1.
    #[serde(default)]
    pub amount_native: Option<String>,
}

use crate::repo::{instruments::InstrumentRow, prices, review_items, transactions};
use rust_decimal::Decimal;

/// Resolve an amount-only trade into (quantity, price). For bibit-sourced
/// funds with a stored NAV quote, derive real units (amount / NAV, 4 dp —
/// Bibit's unit precision). Otherwise: quantity = amount at price 1. Never
/// touches the network; reads only the quote table.
async fn amount_only_qty_price(
    db: &Db,
    ins: &InstrumentRow,
    amount: &str,
    note: &mut Option<String>,
) -> anyhow::Result<(String, String)> {
    let amount_dec = crate::repo::dec(amount)?;
    if ins.price_source.starts_with("bibit:") {
        // Once an instrument has value-based (price = 1) rows, stay on that
        // convention: mixing NAV-derived units with rupiah-as-units rows would
        // make the position unreconcilable (a derived sell could never close a
        // price-1 buy). Edit the legacy rows to real units to unlock derivation.
        if transactions::has_price_one_txn(db, ins.id).await? {
            append_note(note, "dicatat nominal di harga 1 agar konsisten dengan transaksi sebelumnya");
            return Ok((amount_dec.normalize().to_string(), "1".to_string()));
        }
        if let Some(lp) = prices::latest(db, ins.id).await? {
            if lp.source == "bibit" && lp.price > Decimal::ZERO {
                let qty = (amount_dec / lp.price).round_dp(4);
                append_note(note, &format!("unit dihitung dari NAV {} per {}", lp.price.normalize(), lp.as_of));
                return Ok((qty.normalize().to_string(), lp.price.normalize().to_string()));
            }
        }
        append_note(note, "NAV belum tersedia; dicatat nominal di harga 1");
    }
    Ok((amount_dec.normalize().to_string(), "1".to_string()))
}

fn append_note(note: &mut Option<String>, msg: &str) {
    match note {
        Some(n) => {
            n.push_str(" (");
            n.push_str(msg);
            n.push(')');
        }
        None => *note = Some(format!("({msg})")),
    }
}

/// Confirm a review item: build a ledger transaction from the payload and mark the item confirmed.
/// FX fields default from the latest USD/IDR rate when absent. Returns the new txn id.
pub async fn confirm(db: &Db, item_id: i64, p: &ConfirmPayload) -> anyhow::Result<i64> {
    let item = crate::repo::review_items::get(db, item_id).await?;
    if item.status != "pending" {
        return Err(anyhow::anyhow!("review item {item_id} is already {}", item.status));
    }
    let ins = crate::repo::instruments::get(db, p.instrument_id).await
        .map_err(|_| anyhow::anyhow!("unknown instrument_id {}", p.instrument_id))?;
    crate::repo::accounts::get(db, p.account_id).await
        .map_err(|_| anyhow::anyhow!("unknown account_id {}", p.account_id))?;
    let usd_idr = prices::latest_fx(db, "USD", "IDR").await?.unwrap_or(Decimal::ONE);
    let fx_to_idr = p.fx_to_idr.clone().unwrap_or_else(|| usd_idr.to_string());
    let fx_to_usd = p.fx_to_usd.clone().unwrap_or_else(|| "1".to_string());

    // Amount-only mutual fund trades (e.g. Bibit "Order" screenshots) carry a
    // total IDR amount but no units/NAV. With a stored NAV quote (bibit:* price
    // source, refreshed daily) we derive real units: quantity = amount / NAV at
    // price = NAV. Without one we fall back to quantity = amount at price 1,
    // which values the position at cost via the avg_cost fallback
    // (service/portfolio.rs).
    let mut note = p.note.clone();
    let (quantity, price_native) = if p.quantity.trim().is_empty() && p.price_native.trim().is_empty() {
        let amount = p.amount_native.as_deref().map(str::trim).filter(|a| !a.is_empty());
        match (amount, p.entry_type.as_str()) {
            (Some(a), "buy" | "sell") => amount_only_qty_price(db, &ins, a, &mut note).await?,
            _ => return Err(anyhow::anyhow!(
                "quantity/price or amount_native is required for a {} entry",
                p.entry_type
            )),
        }
    } else {
        (p.quantity.clone(), p.price_native.clone())
    };

    let nt = transactions::NewTransaction {
        account_id: p.account_id,
        instrument_id: p.instrument_id,
        txn_type: p.entry_type.clone(),
        executed_at: chrono::DateTime::parse_from_rfc3339(&p.executed_at)
            .map_err(|e| anyhow::anyhow!("bad executed_at: {e}"))?
            .with_timezone(&chrono::Utc),
        quantity,
        price_native,
        fee_native: p.fee_native.clone(),
        currency: p.currency.clone(),
        fx_to_idr,
        fx_to_usd,
        note,
        source: None,
        external_id: None,
    };
    let txn = transactions::create(db, &nt).await?;
    review_items::mark_confirmed(db, item_id, txn.id).await?;
    Ok(txn.id)
}

pub async fn reject(db: &Db, item_id: i64) -> anyhow::Result<()> {
    let item = crate::repo::review_items::get(db, item_id).await?;
    if item.status != "pending" {
        return Err(anyhow::anyhow!("review item {item_id} is already {}", item.status));
    }
    review_items::mark_rejected(db, item_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{accounts, instruments, review_items, transactions};
    use rust_decimal_macros::dec;

    async fn seed(db: &Db) -> (i64, i64) {
        let a = accounts::create(db, &accounts::NewAccount { name:"M".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let i = instruments::create(db, &instruments::NewInstrument { symbol:"BTC".into(), name:"B".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:None, price_source:"manual".into(), decimals:Some(8), note:None }).await.unwrap();
        (a.id, i.id)
    }

    #[tokio::test]
    async fn confirm_inserts_ledger_txn_and_marks_confirmed() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed(&db).await;
        crate::repo::prices::upsert_fx(&db, "USD", "IDR", dec!(16000), "2026-01-01").await.unwrap();
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"f.png", source_path:"p",
            doc_type:"trade_confirmation", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();

        let payload = ConfirmPayload {
            account_id, instrument_id, entry_type:"buy".into(),
            executed_at:"2026-01-02T00:00:00Z".into(), quantity:"1".into(), price_native:"100".into(),
            fee_native:None, currency:"USD".into(), fx_to_idr:None, fx_to_usd:None, note:None,
            amount_native:None,
        };
        let txn_id = confirm(&db, item.id, &payload).await.unwrap();
        assert!(txn_id > 0);
        // ledger now has the transaction
        let txns = transactions::list_all(&db).await.unwrap();
        assert_eq!(txns.len(), 1);
        assert_eq!(txns[0].fx_to_idr, dec!(16000)); // defaulted from latest_fx
        // item marked confirmed with txn id
        let reloaded = review_items::get(&db, item.id).await.unwrap();
        assert_eq!(reloaded.status, "confirmed");
        assert_eq!(reloaded.created_txn_id, Some(txn_id));
    }

    #[tokio::test]
    async fn double_confirm_is_rejected_and_inserts_one_txn() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed(&db).await;
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"f.png", source_path:"p",
            doc_type:"trade_confirmation", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        let payload = ConfirmPayload { account_id, instrument_id, entry_type:"buy".into(),
            executed_at:"2026-01-02T00:00:00Z".into(), quantity:"1".into(), price_native:"100".into(),
            fee_native:None, currency:"USD".into(), fx_to_idr:None, fx_to_usd:None, note:None,
            amount_native:None, };
        confirm(&db, item.id, &payload).await.unwrap();
        assert!(confirm(&db, item.id, &payload).await.is_err()); // second confirm refused
        assert_eq!(crate::repo::transactions::list_all(&db).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn reject_after_confirm_is_refused() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed(&db).await;
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"f.png", source_path:"p",
            doc_type:"trade_confirmation", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        let payload = ConfirmPayload { account_id, instrument_id, entry_type:"buy".into(),
            executed_at:"2026-01-02T00:00:00Z".into(), quantity:"1".into(), price_native:"100".into(),
            fee_native:None, currency:"USD".into(), fx_to_idr:None, fx_to_usd:None, note:None,
            amount_native:None, };
        confirm(&db, item.id, &payload).await.unwrap();
        assert!(reject(&db, item.id).await.is_err());
    }

    #[tokio::test]
    async fn reject_marks_rejected_without_ledger_row() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (_a, _i) = seed(&db).await;
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"f.png", source_path:"p",
            doc_type:"holdings_snapshot", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:None, suggested_account_id:None,
        }).await.unwrap();
        reject(&db, item.id).await.unwrap();
        assert_eq!(review_items::get(&db, item.id).await.unwrap().status, "rejected");
        assert_eq!(transactions::list_all(&db).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn confirm_amount_only_buy_uses_amount_as_quantity_at_price_one() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed(&db).await;
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"asdc.jpeg", source_path:"p",
            doc_type:"txn_history", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        let payload = ConfirmPayload {
            account_id, instrument_id, entry_type:"buy".into(),
            executed_at:"2026-06-05T00:00:00Z".into(), quantity:"".into(), price_native:"".into(),
            fee_native:None, currency:"IDR".into(), fx_to_idr:None, fx_to_usd:None, note:None,
            amount_native:Some("13000000".into()),
        };
        let txn_id = confirm(&db, item.id, &payload).await.unwrap();
        let txns = transactions::list_all(&db).await.unwrap();
        assert_eq!(txns.len(), 1);
        assert_eq!(txns[0].quantity, dec!(13000000));
        assert_eq!(txns[0].price_native, dec!(1));
        assert_eq!(review_items::get(&db, item.id).await.unwrap().created_txn_id, Some(txn_id));
    }

    #[tokio::test]
    async fn confirm_amount_only_sell_also_maps() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed(&db).await;
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"asdc.jpeg", source_path:"p",
            doc_type:"txn_history", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        let payload = ConfirmPayload {
            account_id, instrument_id, entry_type:"sell".into(),
            executed_at:"2026-06-05T00:00:00Z".into(), quantity:"".into(), price_native:"".into(),
            fee_native:None, currency:"IDR".into(), fx_to_idr:None, fx_to_usd:None, note:None,
            amount_native:Some("5000000".into()),
        };
        confirm(&db, item.id, &payload).await.unwrap();
        let txns = transactions::list_all(&db).await.unwrap();
        assert_eq!(txns[0].quantity, dec!(5000000));
        assert_eq!(txns[0].price_native, dec!(1));
        assert_eq!(txns[0].txn_type, crate::domain::models::TxnType::Sell);
    }

    #[tokio::test]
    async fn confirm_without_quantity_price_or_amount_errors_clearly() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed(&db).await;
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"f.png", source_path:"p",
            doc_type:"txn_history", needs_attention:true, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        let payload = ConfirmPayload {
            account_id, instrument_id, entry_type:"buy".into(),
            executed_at:"2026-06-05T00:00:00Z".into(), quantity:"".into(), price_native:"".into(),
            fee_native:None, currency:"IDR".into(), fx_to_idr:None, fx_to_usd:None, note:None,
            amount_native:None,
        };
        let err = confirm(&db, item.id, &payload).await.unwrap_err();
        assert!(err.to_string().contains("amount_native"), "unhelpful message: {err}");
        // nothing persisted, item still pending
        assert_eq!(transactions::list_all(&db).await.unwrap().len(), 0);
        assert_eq!(review_items::get(&db, item.id).await.unwrap().status, "pending");
    }

    #[tokio::test]
    async fn confirm_amount_only_dividend_is_refused() {
        // The amount->quantity convention applies to buy/sell only.
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed(&db).await;
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"f.png", source_path:"p",
            doc_type:"txn_history", needs_attention:true, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        let payload = ConfirmPayload {
            account_id, instrument_id, entry_type:"dividend".into(),
            executed_at:"2026-06-05T00:00:00Z".into(), quantity:"".into(), price_native:"".into(),
            fee_native:None, currency:"IDR".into(), fx_to_idr:None, fx_to_usd:None, note:None,
            amount_native:Some("100000".into()),
        };
        assert!(confirm(&db, item.id, &payload).await.is_err());
        assert_eq!(transactions::list_all(&db).await.unwrap().len(), 0);
    }

    /// A Bibit-sourced mutual fund instrument + goal account.
    async fn seed_fund(db: &Db) -> (i64, i64) {
        let a = accounts::create(db, &accounts::NewAccount { name:"Pendidikan Noah".into(), account_type:"manual".into(), institution:None, native_currency:"IDR".into(), note:None }).await.unwrap();
        let i = instruments::create(db, &instruments::NewInstrument { symbol:"RD1436".into(), name:"Sucorinvest Bond Fund".into(), instrument_type:"mutual_fund".into(), native_currency:"IDR".into(), category_id:None, price_source:"bibit:RD1436".into(), decimals:Some(4), note:None }).await.unwrap();
        (a.id, i.id)
    }

    fn amount_only_payload(account_id: i64, instrument_id: i64, entry_type: &str, amount: &str) -> ConfirmPayload {
        ConfirmPayload {
            account_id, instrument_id, entry_type: entry_type.into(),
            executed_at:"2026-06-05T00:00:00Z".into(), quantity:"".into(), price_native:"".into(),
            fee_native:None, currency:"IDR".into(), fx_to_idr:None, fx_to_usd:None, note:None,
            amount_native:Some(amount.into()),
        }
    }

    #[tokio::test]
    async fn amount_only_buy_with_stored_nav_derives_units() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed_fund(&db).await;
        crate::repo::prices::upsert_latest(&db, instrument_id, dec!(1697.22), "IDR", "bibit", "2026-06-04").await.unwrap();
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"asdc.jpeg", source_path:"p",
            doc_type:"txn_history", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        confirm(&db, item.id, &amount_only_payload(account_id, instrument_id, "buy", "13000000")).await.unwrap();
        let txns = transactions::list_all(&db).await.unwrap();
        assert_eq!(txns[0].quantity, (dec!(13000000) / dec!(1697.22)).round_dp(4));
        assert_eq!(txns[0].price_native, dec!(1697.22));
        let note = txns[0].note.clone().unwrap_or_default();
        assert!(note.contains("NAV 1697.22"), "note should record the NAV used: {note}");
        assert!(note.contains("2026-06-04"), "note should record the NAV date: {note}");
    }

    #[tokio::test]
    async fn amount_only_buy_without_nav_falls_back_to_price_one() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed_fund(&db).await; // no quote stored
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"asdc.jpeg", source_path:"p",
            doc_type:"txn_history", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        confirm(&db, item.id, &amount_only_payload(account_id, instrument_id, "buy", "13000000")).await.unwrap();
        let txns = transactions::list_all(&db).await.unwrap();
        assert_eq!(txns[0].quantity, dec!(13000000));
        assert_eq!(txns[0].price_native, dec!(1));
        let note = txns[0].note.clone().unwrap_or_default();
        assert!(note.contains("NAV belum tersedia"), "fallback should be noted: {note}");
    }

    #[tokio::test]
    async fn amount_only_sell_with_nav_derives_units_too() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed_fund(&db).await;
        crate::repo::prices::upsert_latest(&db, instrument_id, dec!(1617.0896), "IDR", "bibit", "2026-06-04").await.unwrap();
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"asdc.jpeg", source_path:"p",
            doc_type:"txn_history", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        confirm(&db, item.id, &amount_only_payload(account_id, instrument_id, "sell", "5000000")).await.unwrap();
        let txns = transactions::list_all(&db).await.unwrap();
        assert_eq!(txns[0].quantity, (dec!(5000000) / dec!(1617.0896)).round_dp(4));
        assert_eq!(txns[0].price_native, dec!(1617.0896));
    }

    #[tokio::test]
    async fn derivation_skipped_when_instrument_has_price_one_history() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed_fund(&db).await;
        // First confirm happened before any NAV existed -> price=1 row.
        let item1 = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"a.jpeg", source_path:"p",
            doc_type:"txn_history", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        confirm(&db, item1.id, &amount_only_payload(account_id, instrument_id, "buy", "13000000")).await.unwrap();
        // NAV arrives later; a second confirm must NOT switch conventions.
        crate::repo::prices::upsert_latest(&db, instrument_id, dec!(1697.22), "IDR", "bibit", "2026-06-04").await.unwrap();
        let item2 = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"b.jpeg", source_path:"p",
            doc_type:"txn_history", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        confirm(&db, item2.id, &amount_only_payload(account_id, instrument_id, "sell", "5000000")).await.unwrap();
        let txns = transactions::list_all(&db).await.unwrap();
        let sell = txns.iter().find(|t| t.txn_type == crate::domain::models::TxnType::Sell).unwrap();
        assert_eq!(sell.quantity, dec!(5000000), "must stay value-based");
        assert_eq!(sell.price_native, dec!(1));
        assert!(sell.note.clone().unwrap_or_default().contains("konsisten"), "note should explain the gate");
    }

    #[tokio::test]
    async fn non_bibit_latest_quote_is_not_used_as_nav() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed_fund(&db).await;
        // A stray manual quote must not be mistaken for NAV.
        crate::repo::prices::upsert_latest(&db, instrument_id, dec!(999), "IDR", "manual", "2026-06-04").await.unwrap();
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"a.jpeg", source_path:"p",
            doc_type:"txn_history", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        confirm(&db, item.id, &amount_only_payload(account_id, instrument_id, "buy", "1000000")).await.unwrap();
        let txns = transactions::list_all(&db).await.unwrap();
        assert_eq!(txns[0].quantity, dec!(1000000));
        assert_eq!(txns[0].price_native, dec!(1));
    }

    #[tokio::test]
    async fn amount_only_on_non_bibit_instrument_keeps_price_one_without_note() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed(&db).await; // crypto, price_source "manual"
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"f.png", source_path:"p",
            doc_type:"txn_history", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        confirm(&db, item.id, &amount_only_payload(account_id, instrument_id, "buy", "750000")).await.unwrap();
        let txns = transactions::list_all(&db).await.unwrap();
        assert_eq!(txns[0].quantity, dec!(750000));
        assert_eq!(txns[0].price_native, dec!(1));
        assert_eq!(txns[0].note, None, "non-bibit fallback must not add a note");
    }
}
