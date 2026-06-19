use crate::db::Db;
use crate::ingestion::extract::ExtractedEntry;
use crate::repo::review_items::ReviewItemRow;
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

/// Coerce a payload date into RFC3339: full RFC3339 passes through,
/// "YYYY-MM-DDTHH:MM" and date-only values are assumed UTC.
pub fn to_rfc3339(s: &str) -> Option<String> {
    if chrono::DateTime::parse_from_rfc3339(s).is_ok() {
        return Some(s.to_string());
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M") {
        return Some(format!("{}Z", dt.format("%Y-%m-%dT%H:%M:%S")));
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(format!("{d}T00:00:00Z"));
    }
    None
}

/// Build the ConfirmPayload for one-tap confirmation, or explain (in user
/// language) why the item must be completed in the web UI instead.
pub fn build_confirm_payload(item: &ReviewItemRow) -> Result<ConfirmPayload, String> {
    if item.needs_attention != 0 {
        return Err("item ini perlu dicek manual".into());
    }
    let account_id = item.suggested_account_id.ok_or("akun belum dikenali")?;
    let instrument_id = item.suggested_instrument_id.ok_or("instrumen belum dikenali")?;
    let entry: ExtractedEntry = serde_json::from_str(&item.payload_json)
        .map_err(|e| format!("payload tidak terbaca: {e}"))?;
    // Amount-only fund entries (an IDR amount, no units/NAV — e.g. Bibit) are
    // one-tap confirmable: confirm() derives NAV units when a stored bibit
    // quote exists, else records quantity = amount at price 1, for buy/sell.
    let amount_only = entry.quantity.is_none()
        && entry.price_native.is_none()
        && entry.amount_native.is_some()
        && matches!(entry.entry_type.as_str(), "buy" | "sell");
    let (quantity, price_native) = if amount_only {
        (String::new(), String::new())
    } else {
        (
            entry.quantity.ok_or("jumlah tidak ada")?,
            entry.price_native.ok_or("harga tidak ada")?,
        )
    };
    let currency = entry.currency.ok_or("mata uang tidak ada")?;
    let executed_at = match &entry.executed_at {
        Some(raw) => to_rfc3339(raw).ok_or_else(|| format!("tanggal tidak terbaca: {raw}"))?,
        None => chrono::Utc::now().to_rfc3339(),
    };
    Ok(ConfirmPayload {
        account_id,
        instrument_id,
        entry_type: entry.entry_type,
        executed_at,
        quantity,
        price_native,
        fee_native: entry.fee_native,
        currency,
        fx_to_idr: None,
        fx_to_usd: None,
        note: entry.note,
        amount_native: entry.amount_native,
    })
}

use crate::repo::{prices, review_items, transactions};
use rust_decimal::Decimal;

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

    let mut note = p.note.clone();
    let has_qp = !p.quantity.trim().is_empty() || !p.price_native.trim().is_empty();
    let (quantity, price_native) = if has_qp {
        (p.quantity.clone(), p.price_native.clone())
    } else {
        let q = (!p.quantity.trim().is_empty()).then(|| p.quantity.as_str());
        let pr = (!p.price_native.trim().is_empty()).then(|| p.price_native.as_str());
        let amt = p.amount_native.as_deref().map(str::trim).filter(|a| !a.is_empty());
        if amt.is_none() {
            return Err(anyhow::anyhow!(
                "quantity/price or amount_native is required for a {} entry",
                p.entry_type
            ));
        }
        match crate::service::txn_entry::resolve_qty_price(
            db, &ins, &p.entry_type, q, pr, amt, /* allow_price_one_fallback */ true, &mut note,
        ).await {
            Ok(pair) => pair,
            Err(crate::service::txn_entry::ResolveError::NeedNavOrUnits) => {
                return Err(anyhow::anyhow!("butuh NAV atau jumlah unit untuk {}", p.entry_type));
            }
            Err(crate::service::txn_entry::ResolveError::Other(e)) => return Err(e),
        }
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

    // Bank-statement entries also feed the cashflow/Budget view. We read the
    // category + dedup ref from the stored extraction (the user-facing
    // ConfirmPayload does not carry them), and key direction off entry_type.
    if item.doc_type == "bank_statement_bca"
        && matches!(p.entry_type.as_str(), "deposit" | "withdrawal")
    {
        if let Err(e) = write_bank_cashflow(db, &item, p).await {
            // Don't fail the whole confirm if the cashflow mirror fails; the
            // txn is the source of truth. Surface it loudly for follow-up.
            tracing::warn!("confirm: cashflow mirror failed for item {}: {e:#}", item.id);
        }
    }

    review_items::mark_confirmed(db, item_id, txn.id).await?;
    Ok(txn.id)
}

/// Mirror a confirmed bank-statement deposit/withdrawal into the cashflow table.
/// Idempotent on `(source, external_ref)` so re-imports never double-count.
async fn write_bank_cashflow(
    db: &Db,
    item: &crate::repo::review_items::ReviewItemRow,
    p: &ConfirmPayload,
) -> anyhow::Result<()> {
    use crate::repo::{cashflow, cashflow_categories};
    let stored: crate::ingestion::extract::ExtractedEntry =
        serde_json::from_str(&item.payload_json)?;
    let external_ref = stored.external_ref
        .ok_or_else(|| anyhow::anyhow!("bank_statement item missing external_ref"))?;

    let direction = match p.entry_type.as_str() {
        "deposit"    => "in",
        "withdrawal" => "out",
        other => anyhow::bail!("write_bank_cashflow: unexpected entry_type {other:?}"),
    };
    let amount = p.amount_native.clone()
        .ok_or_else(|| anyhow::anyhow!("bank cashflow needs amount_native"))?;
    let occurred_on = to_rfc3339(&p.executed_at)
        .unwrap_or_else(|| p.executed_at.clone());
    let occurred_on = occurred_on.get(0..10).unwrap_or(&occurred_on).to_string();

    let category_id = match stored.cashflow_category.as_deref() {
        Some(name) if !name.is_empty() => {
            let kind = if direction == "in" { "income" } else { "expense" };
            // ensure_by_name matches on name only; `kind` is fixed at first creation
            // (first-write-wins). That is fine because cashflow reporting keys on the
            // cashflow row's own `direction` ("in"/"out"), not the category kind.
            Some(cashflow_categories::ensure_by_name(db, name, kind).await?.id)
        }
        _ => None,
    };

    cashflow::insert_sourced(
        db,
        &cashflow::NewCashflow {
            account_id: Some(p.account_id),
            occurred_on,
            direction: direction.to_string(),
            amount,
            currency: p.currency.clone(),
            category_id,
            note: p.note.clone().or(stored.note),
        },
        "bank_statement_bca",
        &external_ref,
    ).await?;
    Ok(())
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

    fn review_item(payload_json: &str) -> crate::repo::review_items::ReviewItemRow {
        crate::repo::review_items::ReviewItemRow {
            id: 42,
            batch_id: "tg-1".into(),
            source_kind: "image".into(),
            source_filename: "telegram-photo.jpg".into(),
            source_path: "".into(),
            doc_type: "txn_history".into(),
            status: "pending".into(),
            needs_attention: 0,
            payload_json: payload_json.into(),
            raw_llm_json: "{}".into(),
            suggested_instrument_id: Some(9),
            suggested_account_id: Some(2),
            created_txn_id: None,
            created_at: "2026-06-05T00:00:00Z".into(),
            confirmed_at: None,
        }
    }

    const FULL_PAYLOAD: &str = r#"{
        "entry_type": "buy", "symbol": "BTC", "quantity": "0.00128248",
        "price_native": "1169608882", "fee_native": "0", "currency": "IDR",
        "executed_at": "2026-06-04", "confidence": 0.95
    }"#;

    const AMOUNT_ONLY_PAYLOAD: &str = r#"{
        "entry_type": "buy", "instrument_name": "Sucorinvest Bond Fund",
        "amount_native": "13000000", "currency": "IDR", "confidence": 0.72
    }"#;

    #[test]
    fn coerces_dates_to_rfc3339() {
        assert_eq!(to_rfc3339("2026-06-04T11:32:00Z").as_deref(), Some("2026-06-04T11:32:00Z"));
        assert_eq!(to_rfc3339("2026-06-04T11:32").as_deref(), Some("2026-06-04T11:32:00Z"));
        assert_eq!(to_rfc3339("2026-06-04").as_deref(), Some("2026-06-04T00:00:00Z"));
        assert_eq!(to_rfc3339("kemarin"), None);
    }

    #[test]
    fn full_items_build_a_confirm_payload() {
        let payload = build_confirm_payload(&review_item(FULL_PAYLOAD)).expect("confirmable");
        assert_eq!(payload.account_id, 2);
        assert_eq!(payload.instrument_id, 9);
        assert_eq!(payload.entry_type, "buy");
        assert_eq!(payload.quantity, "0.00128248");
        assert_eq!(payload.executed_at, "2026-06-04T00:00:00Z");
        assert_eq!(payload.currency, "IDR");
    }

    #[test]
    fn attention_items_are_not_confirmable() {
        let mut item = review_item(FULL_PAYLOAD);
        item.needs_attention = 1;
        assert!(build_confirm_payload(&item).is_err());
    }

    #[test]
    fn items_without_suggestions_are_not_confirmable() {
        let mut item = review_item(FULL_PAYLOAD);
        item.suggested_account_id = None;
        assert!(build_confirm_payload(&item).is_err());

        let mut item = review_item(FULL_PAYLOAD);
        item.suggested_instrument_id = None;
        assert!(build_confirm_payload(&item).is_err());
    }

    #[test]
    fn items_missing_core_fields_are_not_confirmable() {
        let payload = r#"{ "entry_type": "buy", "symbol": "BTC", "confidence": 0.95 }"#;
        assert!(build_confirm_payload(&review_item(payload)).is_err());
    }

    #[test]
    fn amount_only_fund_items_build_a_confirm_payload() {
        let payload =
            build_confirm_payload(&review_item(AMOUNT_ONLY_PAYLOAD)).expect("confirmable");
        assert_eq!(payload.quantity, "");
        assert_eq!(payload.price_native, "");
        assert_eq!(payload.amount_native.as_deref(), Some("13000000"));
        assert_eq!(payload.currency, "IDR");
    }

    #[test]
    fn amount_only_dividend_is_not_confirmable() {
        let payload = r#"{ "entry_type": "dividend", "amount_native": "100000", "currency": "IDR", "confidence": 0.9 }"#;
        assert!(build_confirm_payload(&review_item(payload)).is_err());
    }

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

    #[tokio::test]
    async fn confirm_bca_withdrawal_creates_txn_and_cashflow() {
        use crate::repo::{cashflow, review_items};
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let now = chrono::Utc::now().to_rfc3339();

        // Seed a Cash instrument and a BCA account (FKs for the txn).
        let account_id = sqlx::query(
            "INSERT INTO account (name, account_type, native_currency, created_at) VALUES (?,?,?,?)")
            .bind("BCA").bind("bank").bind("IDR").bind(&now)
            .execute(&db).await.unwrap().last_insert_rowid();
        let instrument_id = sqlx::query(
            "INSERT INTO instrument (symbol, name, instrument_type, native_currency, price_source) VALUES (?,?,?,?,?)")
            .bind("CASHIDR").bind("Cash IDR").bind("cash").bind("IDR").bind("manual")
            .execute(&db).await.unwrap().last_insert_rowid();

        // Stage a BCA withdrawal review item carrying provenance in its payload.
        let payload = serde_json::json!({
            "entry_type": "withdrawal",
            "currency": "IDR",
            "executed_at": "2026-05-01T00:00:00Z",
            "amount_native": "242000.00",
            "cashflow_category": "Transfer",
            "external_ref": "bca:8415525237:2026-05-01:242000.00:0",
            "note": "TRSF E-BANKING DB PT Moratelin"
        }).to_string();
        let item = review_items::create(&db, &crate::repo::review_items::NewReviewItem {
            batch_id: "b1", source_kind: "pdf", source_filename: "s.pdf", source_path: "",
            doc_type: "bank_statement_bca", needs_attention: false,
            payload_json: &payload, raw_llm_json: "{}",
            suggested_instrument_id: None, suggested_account_id: None,
        }).await.unwrap();

        let p = ConfirmPayload {
            account_id, instrument_id, entry_type: "withdrawal".into(),
            executed_at: "2026-05-01T00:00:00Z".into(),
            quantity: String::new(), price_native: String::new(),
            fee_native: None, currency: "IDR".into(),
            fx_to_idr: Some("1".into()), fx_to_usd: Some("1".into()),
            note: None, amount_native: Some("242000.00".into()),
        };
        let txn_id = confirm(&db, item.id, &p).await.unwrap();
        assert!(txn_id > 0);

        let rows = cashflow::list_all(&db).await.unwrap();
        assert_eq!(rows.len(), 1, "one cashflow row created");
        assert_eq!(rows[0].direction, "out");
        assert_eq!(rows[0].amount, "242000.00");
        assert_eq!(rows[0].source.as_deref(), Some("bank_statement_bca"));
        assert_eq!(rows[0].external_ref.as_deref(), Some("bca:8415525237:2026-05-01:242000.00:0"));
        assert!(rows[0].category_id.is_some(), "Transfer category attached");
    }

    #[tokio::test]
    async fn confirm_bca_deposit_creates_txn_and_cashflow() {
        use crate::repo::{cashflow, cashflow_categories, review_items};
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let now = chrono::Utc::now().to_rfc3339();

        // Seed a Cash instrument and a BCA account (FKs for the txn).
        let account_id = sqlx::query(
            "INSERT INTO account (name, account_type, native_currency, created_at) VALUES (?,?,?,?)")
            .bind("BCA").bind("bank").bind("IDR").bind(&now)
            .execute(&db).await.unwrap().last_insert_rowid();
        let instrument_id = sqlx::query(
            "INSERT INTO instrument (symbol, name, instrument_type, native_currency, price_source) VALUES (?,?,?,?,?)")
            .bind("CASHIDR").bind("Cash IDR").bind("cash").bind("IDR").bind("manual")
            .execute(&db).await.unwrap().last_insert_rowid();

        // Stage a BCA deposit review item carrying provenance in its payload.
        let payload = serde_json::json!({
            "entry_type": "deposit",
            "currency": "IDR",
            "executed_at": "2026-05-12T00:00:00Z",
            "amount_native": "49995500.00",
            "cashflow_category": "Transfer",
            "external_ref": "bca:8415525237:2026-05-12:49995500.00:0",
            "note": "TRSF MASUK"
        }).to_string();
        let item = review_items::create(&db, &crate::repo::review_items::NewReviewItem {
            batch_id: "b2", source_kind: "pdf", source_filename: "s.pdf", source_path: "",
            doc_type: "bank_statement_bca", needs_attention: false,
            payload_json: &payload, raw_llm_json: "{}",
            suggested_instrument_id: None, suggested_account_id: None,
        }).await.unwrap();

        let p = ConfirmPayload {
            account_id, instrument_id, entry_type: "deposit".into(),
            executed_at: "2026-05-12T00:00:00Z".into(),
            quantity: String::new(), price_native: String::new(),
            fee_native: None, currency: "IDR".into(),
            fx_to_idr: Some("1".into()), fx_to_usd: Some("1".into()),
            note: None, amount_native: Some("49995500.00".into()),
        };
        let txn_id = confirm(&db, item.id, &p).await.unwrap();
        assert!(txn_id > 0);

        let rows = cashflow::list_all(&db).await.unwrap();
        assert_eq!(rows.len(), 1, "one cashflow row created");
        assert_eq!(rows[0].direction, "in");
        assert_eq!(rows[0].amount, "49995500.00");
        assert_eq!(rows[0].source.as_deref(), Some("bank_statement_bca"));
        assert_eq!(rows[0].external_ref.as_deref(), Some("bca:8415525237:2026-05-12:49995500.00:0"));
        assert!(rows[0].category_id.is_some(), "Transfer category attached");

        // The "Transfer" category for a deposit must have kind "income".
        // kind is first-write-wins; here the deposit is the first Transfer so kind is "income".
        let cat = cashflow_categories::get(&db, rows[0].category_id.unwrap()).await.unwrap();
        assert_eq!(cat.kind, "income");
    }
}
