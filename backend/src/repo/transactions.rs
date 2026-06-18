use crate::db::Db;
use crate::domain::models::{Transaction, TxnType};
use crate::repo::dec;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::str::FromStr;

#[derive(Debug, Deserialize)]
pub struct NewTransaction {
    pub account_id: i64,
    pub instrument_id: i64,
    pub txn_type: String,
    pub executed_at: DateTime<Utc>,
    pub quantity: String,
    pub price_native: String,
    pub fee_native: Option<String>,
    pub currency: String,
    pub fx_to_idr: String,
    pub fx_to_usd: String,
    pub note: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub external_id: Option<String>,
}

#[derive(sqlx::FromRow)]
struct TxnRowRaw {
    id: i64, account_id: i64, instrument_id: i64, txn_type: String,
    executed_at: String, quantity: String, price_native: String, fee_native: String,
    currency: String, fx_to_idr: String, fx_to_usd: String, note: Option<String>,
}

impl TxnRowRaw {
    fn into_domain(self) -> anyhow::Result<Transaction> {
        Ok(Transaction {
            id: self.id, account_id: self.account_id, instrument_id: self.instrument_id,
            txn_type: TxnType::from_str(&self.txn_type).map_err(|e| anyhow::anyhow!(e))?,
            executed_at: DateTime::parse_from_rfc3339(&self.executed_at)?.with_timezone(&Utc),
            quantity: dec(&self.quantity)?, price_native: dec(&self.price_native)?,
            fee_native: dec(&self.fee_native)?, currency: self.currency,
            fx_to_idr: dec(&self.fx_to_idr)?, fx_to_usd: dec(&self.fx_to_usd)?, note: self.note,
        })
    }
}

pub async fn create(db: &Db, t: &NewTransaction) -> anyhow::Result<Transaction> {
    TxnType::from_str(&t.txn_type).map_err(|e| anyhow::anyhow!(e))?;
    // Validate all decimal fields up-front so a malformed value never persists.
    crate::repo::dec(&t.quantity)?;
    crate::repo::dec(&t.price_native)?;
    if let Some(f) = t.fee_native.as_deref() { crate::repo::dec(f)?; }
    crate::repo::dec(&t.fx_to_idr)?;
    crate::repo::dec(&t.fx_to_usd)?;
    // fx_to_idr converts native -> IDR, so for an IDR transaction it is 1 by
    // identity — normalize here so no caller (manual dialog, ingest confirm,
    // sync) can persist a bogus rate that inflates IDR aggregations. fx_to_usd
    // is derived from the latest USD/IDR rate when one is known.
    let (fx_to_idr, fx_to_usd) = if t.currency == "IDR" {
        let usd_idr = crate::repo::prices::latest_fx(db, "USD", "IDR").await?;
        let to_usd = match usd_idr {
            Some(rate) if !rate.is_zero() => (rust_decimal::Decimal::ONE / rate).to_string(),
            _ => t.fx_to_usd.clone(),
        };
        ("1".to_string(), to_usd)
    } else {
        (t.fx_to_idr.clone(), t.fx_to_usd.clone())
    };
    let now = Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO txn (account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, fx_to_idr, fx_to_usd, note, created_at, source, external_id) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(t.account_id).bind(t.instrument_id).bind(&t.txn_type)
        .bind(t.executed_at.to_rfc3339()).bind(&t.quantity).bind(&t.price_native)
        .bind(t.fee_native.clone().unwrap_or_else(|| "0".into()))
        .bind(&t.currency).bind(&fx_to_idr).bind(&fx_to_usd).bind(&t.note).bind(&now)
        .bind(&t.source).bind(&t.external_id)
        .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<Transaction> {
    let raw = sqlx::query_as::<_, TxnRowRaw>("SELECT id, account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, fx_to_idr, fx_to_usd, note FROM txn WHERE id = ?")
        .bind(id).fetch_one(db).await?;
    raw.into_domain()
}

pub async fn list_all(db: &Db) -> anyhow::Result<Vec<Transaction>> {
    let raws = sqlx::query_as::<_, TxnRowRaw>("SELECT id, account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, fx_to_idr, fx_to_usd, note FROM txn ORDER BY executed_at")
        .fetch_all(db).await?;
    raws.into_iter().map(|r| r.into_domain()).collect()
}

#[allow(dead_code)] // exercised by tests; not yet called from non-test code
pub async fn list_for_instrument(db: &Db, instrument_id: i64) -> anyhow::Result<Vec<Transaction>> {
    let raws = sqlx::query_as::<_, TxnRowRaw>("SELECT id, account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, fx_to_idr, fx_to_usd, note FROM txn WHERE instrument_id = ? ORDER BY executed_at")
        .bind(instrument_id).fetch_all(db).await?;
    raws.into_iter().map(|r| r.into_domain()).collect()
}

/// Recent transactions, newest first, optionally filtered by instrument/account.
pub async fn list_recent(
    db: &Db,
    limit: i64,
    instrument_id: Option<i64>,
    account_id: Option<i64>,
) -> anyhow::Result<Vec<Transaction>> {
    let raws = sqlx::query_as::<_, TxnRowRaw>(
        "SELECT id, account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, fx_to_idr, fx_to_usd, note \
         FROM txn \
         WHERE (?1 IS NULL OR instrument_id = ?1) AND (?2 IS NULL OR account_id = ?2) \
         ORDER BY executed_at DESC, id DESC LIMIT ?3")
        .bind(instrument_id).bind(account_id).bind(limit)
        .fetch_all(db).await?;
    raws.into_iter().map(|r| r.into_domain()).collect()
}

/// Patch struct for `update`. All fields are optional; omitted fields retain
/// their current value. Currency changes trigger IDR fx renormalization.
#[derive(Debug, Default)]
pub struct TxnPatch {
    pub account_id: Option<i64>,
    pub instrument_id: Option<i64>,
    pub txn_type: Option<String>,
    pub executed_at: Option<DateTime<Utc>>,
    pub quantity: Option<String>,
    pub price_native: Option<String>,
    pub fee_native: Option<String>,
    pub currency: Option<String>,
    pub note: Option<String>,
}

/// Update selected fields of a transaction. Re-applies IDR fx normalization
/// (fx_to_idr = 1 for IDR) so an edit can never persist a bogus rate.
///
/// Reads the current row, overlays the patch's Some fields, re-validates
/// all decimal fields, re-applies the same IDR fx normalization as `create`,
/// and persists via UPDATE.
pub async fn update(db: &Db, id: i64, patch: &TxnPatch) -> anyhow::Result<Transaction> {
    let current = get(db, id).await?;

    let account_id = patch.account_id.unwrap_or(current.account_id);
    let instrument_id = patch.instrument_id.unwrap_or(current.instrument_id);
    let txn_type = patch
        .txn_type
        .clone()
        .unwrap_or_else(|| current.txn_type.as_str().to_string());
    TxnType::from_str(&txn_type).map_err(|e| anyhow::anyhow!(e))?;

    let executed_at = patch.executed_at.unwrap_or(current.executed_at);
    let quantity = patch
        .quantity
        .clone()
        .unwrap_or_else(|| current.quantity.to_string());
    let price_native = patch
        .price_native
        .clone()
        .unwrap_or_else(|| current.price_native.to_string());
    let fee_native = patch
        .fee_native
        .clone()
        .unwrap_or_else(|| current.fee_native.to_string());
    let currency = patch.currency.clone().unwrap_or(current.currency);
    let note = patch.note.clone().or(current.note);

    crate::repo::dec(&quantity)?;
    crate::repo::dec(&price_native)?;
    crate::repo::dec(&fee_native)?;

    // Mirror the IDR fx normalization from `create` exactly: for IDR transactions
    // fx_to_idr is always 1 by identity, and fx_to_usd is derived from the latest
    // known USD/IDR rate. For non-IDR transactions, keep existing rates.
    let (fx_to_idr, fx_to_usd) = if currency == "IDR" {
        let usd_idr = crate::repo::prices::latest_fx(db, "USD", "IDR").await?;
        let to_usd = match usd_idr {
            Some(rate) if !rate.is_zero() => {
                (rust_decimal::Decimal::ONE / rate).to_string()
            }
            _ => current.fx_to_usd.to_string(),
        };
        ("1".to_string(), to_usd)
    } else {
        (current.fx_to_idr.to_string(), current.fx_to_usd.to_string())
    };

    sqlx::query(
        "UPDATE txn SET account_id=?, instrument_id=?, txn_type=?, executed_at=?, \
         quantity=?, price_native=?, fee_native=?, currency=?, fx_to_idr=?, fx_to_usd=?, \
         note=? WHERE id=?",
    )
    .bind(account_id)
    .bind(instrument_id)
    .bind(&txn_type)
    .bind(executed_at.to_rfc3339())
    .bind(&quantity)
    .bind(&price_native)
    .bind(&fee_native)
    .bind(&currency)
    .bind(&fx_to_idr)
    .bind(&fx_to_usd)
    .bind(&note)
    .bind(id)
    .execute(db)
    .await?;

    get(db, id).await
}

pub async fn delete(db: &Db, id: i64) -> anyhow::Result<()> {
    // Txns confirmed from ingest are referenced by review_item.created_txn_id;
    // clear the reference first or the FK constraint rejects the delete.
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE review_item SET created_txn_id = NULL WHERE created_txn_id = ?")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM txn WHERE id = ?").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// True if the instrument already has a value-based (price = 1) ledger row —
/// the amount-only fallback convention. Used to keep an instrument on ONE
/// convention: once value-based rows exist, NAV derivation must not mix in
/// unit-based rows.
pub async fn has_price_one_txn(db: &Db, instrument_id: i64) -> anyhow::Result<bool> {
    let row = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM txn WHERE instrument_id = ? AND price_native = '1'")
        .bind(instrument_id).fetch_one(db).await?;
    Ok(row.0 > 0)
}

/// Per account that has traded this instrument: (account_id, txn_count,
/// last_executed_at), ordered by count desc then most-recent first. Drives the
/// "infer the account from history" step of ingest account resolution.
pub async fn accounts_for_instrument(
    db: &Db,
    instrument_id: i64,
) -> anyhow::Result<Vec<(i64, i64, String)>> {
    let rows = sqlx::query_as::<_, (i64, i64, String)>(
        "SELECT account_id, COUNT(*) AS cnt, MAX(executed_at) AS last_at \
         FROM txn WHERE instrument_id = ? \
         GROUP BY account_id ORDER BY cnt DESC, last_at DESC",
    )
    .bind(instrument_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Returns the set of external_ids already present for the given source.
pub async fn existing_external_ids(db: &Db, source: &str) -> anyhow::Result<std::collections::HashSet<String>> {
    let rows = sqlx::query_as::<_, (String,)>("SELECT external_id FROM txn WHERE source = ? AND external_id IS NOT NULL")
        .bind(source).fetch_all(db).await?;
    Ok(rows.into_iter().map(|(e,)| e).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{accounts, instruments};
    use rust_decimal_macros::dec as d;

    #[tokio::test]
    async fn idr_transaction_always_stores_fx_to_idr_of_one() {
        // fx_to_idr converts native -> IDR; for an IDR txn it is 1 by identity.
        // The ingest-confirm flow used to default it to the USD/IDR rate, which
        // inflated dividend TTM ~18,000x in insights.
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"IDR".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"BBRI".into(), name:"BRI".into(), instrument_type:"stock_id".into(), native_currency:"IDR".into(), category_id:None, price_source:"manual".into(), decimals:Some(0), note:None }).await.unwrap();
        crate::repo::prices::upsert_fx(&db, "USD", "IDR", d!(18026), "2026-06-04").await.unwrap();

        let nt = NewTransaction { account_id: acc.id, instrument_id: ins.id, txn_type:"dividend".into(),
            executed_at: Utc::now(), quantity:"1700".into(), price_native:"137".into(),
            fee_native: None, currency:"IDR".into(),
            fx_to_idr:"18026".into(), fx_to_usd:"1".into(), // bogus values must be normalized
            note:None, source: None, external_id: None };
        let txn = create(&db, &nt).await.unwrap();

        assert_eq!(txn.fx_to_idr, d!(1));
        assert_eq!(txn.fx_to_usd, d!(1) / d!(18026)); // derived from latest USD/IDR
    }

    #[tokio::test]
    async fn non_idr_transaction_keeps_provided_fx() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"VOO".into(), name:"VOO".into(), instrument_type:"etf".into(), native_currency:"USD".into(), category_id:None, price_source:"manual".into(), decimals:Some(8), note:None }).await.unwrap();
        let nt = NewTransaction { account_id: acc.id, instrument_id: ins.id, txn_type:"buy".into(),
            executed_at: Utc::now(), quantity:"1".into(), price_native:"696".into(),
            fee_native: None, currency:"USD".into(),
            fx_to_idr:"17549".into(), fx_to_usd:"1".into(),
            note:None, source: None, external_id: None };
        let txn = create(&db, &nt).await.unwrap();
        assert_eq!(txn.fx_to_idr, d!(17549)); // historical rate is intentional, keep it
        assert_eq!(txn.fx_to_usd, d!(1));
    }

    #[tokio::test]
    async fn delete_clears_review_item_reference() {
        // Txn created from ingest confirm is referenced by review_item.created_txn_id;
        // delete must clear that reference instead of failing the FK constraint.
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"IDR".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"BMRI".into(), name:"Bank Mandiri".into(), instrument_type:"stock_id".into(), native_currency:"IDR".into(), category_id:None, price_source:"manual".into(), decimals:Some(0), note:None }).await.unwrap();
        let nt = NewTransaction { account_id: acc.id, instrument_id: ins.id, txn_type:"buy".into(),
            executed_at: Utc::now(), quantity:"200".into(), price_native:"3960".into(),
            fee_native: Some("0".into()), currency:"IDR".into(), fx_to_idr:"16000".into(), fx_to_usd:"1".into(), note:None,
            source: None, external_id: None };
        let txn = create(&db, &nt).await.unwrap();

        let item = crate::repo::review_items::create(&db, &crate::repo::review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"f.png", source_path:"p",
            doc_type:"txn_history", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(ins.id), suggested_account_id:Some(acc.id),
        }).await.unwrap();
        crate::repo::review_items::mark_confirmed(&db, item.id, txn.id).await.unwrap();

        delete(&db, txn.id).await.unwrap();

        assert!(list_for_instrument(&db, ins.id).await.unwrap().is_empty());
        let reloaded = crate::repo::review_items::get(&db, item.id).await.unwrap();
        assert_eq!(reloaded.created_txn_id, None);
        assert_eq!(reloaded.status, "confirmed"); // history of the review stays intact
    }

    #[tokio::test]
    async fn insert_and_load_transactions_as_domain() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"BTC".into(), name:"Bitcoin".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:None, price_source:"coingecko:bitcoin".into(), decimals:Some(8), note:None }).await.unwrap();
        let nt = NewTransaction { account_id: acc.id, instrument_id: ins.id, txn_type:"buy".into(),
            executed_at: Utc::now(), quantity:"0.5".into(), price_native:"100".into(),
            fee_native: Some("1".into()), currency:"USD".into(), fx_to_idr:"16000".into(), fx_to_usd:"1".into(), note:None,
            source: None, external_id: None };
        create(&db, &nt).await.unwrap();
        let all = list_for_instrument(&db, ins.id).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].quantity, d!(0.5));
        assert_eq!(all[0].fee_native, d!(1));
    }

    #[tokio::test]
    async fn create_rejects_bad_decimal_without_persisting() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"BTC".into(), name:"B".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:None, price_source:"manual".into(), decimals:Some(8), note:None }).await.unwrap();
        let bad = NewTransaction { account_id: acc.id, instrument_id: ins.id, txn_type:"buy".into(),
            executed_at: Utc::now(), quantity:"not_a_number".into(), price_native:"100".into(),
            fee_native: None, currency:"USD".into(), fx_to_idr:"16000".into(), fx_to_usd:"1".into(), note:None,
            source: None, external_id: None };
        assert!(create(&db, &bad).await.is_err());
        // No row must have been persisted.
        assert_eq!(list_all(&db).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn has_price_one_txn_detects_value_based_rows() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount {
            name:"A".into(), account_type:"manual".into(), institution:None,
            native_currency:"IDR".into(), note:None,
        }).await.unwrap();
        let ins1 = instruments::create(&db, &instruments::NewInstrument {
            symbol:"RD1436".into(), name:"Sucorinvest Bond Fund".into(),
            instrument_type:"mutual_fund".into(), native_currency:"IDR".into(),
            category_id:None, price_source:"bibit:RD1436".into(), decimals:Some(4), note:None,
        }).await.unwrap();
        let ins2 = instruments::create(&db, &instruments::NewInstrument {
            symbol:"RD831".into(), name:"Majoris".into(),
            instrument_type:"mutual_fund".into(), native_currency:"IDR".into(),
            category_id:None, price_source:"bibit:RD831".into(), decimals:Some(4), note:None,
        }).await.unwrap();

        // No transactions yet — must be false for ins1.
        assert!(!has_price_one_txn(&db, ins1.id).await.unwrap());

        // Insert a price=1 (value-based) txn for ins1.
        let nt = NewTransaction {
            account_id: acc.id, instrument_id: ins1.id, txn_type:"buy".into(),
            executed_at: Utc::now(), quantity:"13000000".into(), price_native:"1".into(),
            fee_native: None, currency:"IDR".into(), fx_to_idr:"1".into(), fx_to_usd:"1".into(),
            note:None, source:None, external_id:None,
        };
        create(&db, &nt).await.unwrap();
        assert!(has_price_one_txn(&db, ins1.id).await.unwrap());

        // ins2 only has a NAV-priced txn — must be false for ins2.
        let nt2 = NewTransaction {
            account_id: acc.id, instrument_id: ins2.id, txn_type:"buy".into(),
            executed_at: Utc::now(), quantity:"7658.8934".into(), price_native:"1697.22".into(),
            fee_native: None, currency:"IDR".into(), fx_to_idr:"1".into(), fx_to_usd:"1".into(),
            note:None, source:None, external_id:None,
        };
        create(&db, &nt2).await.unwrap();
        assert!(!has_price_one_txn(&db, ins2.id).await.unwrap());
    }

    #[tokio::test]
    async fn accounts_for_instrument_orders_by_count_then_recency() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc_a = accounts::create(&db, &accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let acc_b = accounts::create(&db, &accounts::NewAccount { name:"B".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let acc_c = accounts::create(&db, &accounts::NewAccount { name:"C".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"QQQM".into(), name:"Invesco NASDAQ 100 ETF".into(), instrument_type:"etf".into(), native_currency:"USD".into(), category_id:None, price_source:"manual".into(), decimals:Some(8), note:None }).await.unwrap();

        let buy = |account_id: i64, when: DateTime<Utc>| NewTransaction {
            account_id, instrument_id: ins.id, txn_type:"buy".into(), executed_at: when,
            quantity:"1".into(), price_native:"100".into(), fee_native:None, currency:"USD".into(),
            fx_to_idr:"16000".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None,
        };
        let at = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
        // A and B both have 2 txns (tie on count); A's most-recent is later than B's.
        // C has the single newest txn overall — must still sort last on count.
        create(&db, &buy(acc_a.id, at("2026-01-01T00:00:00Z"))).await.unwrap();
        create(&db, &buy(acc_a.id, at("2026-04-01T00:00:00Z"))).await.unwrap();
        create(&db, &buy(acc_b.id, at("2026-02-01T00:00:00Z"))).await.unwrap();
        create(&db, &buy(acc_b.id, at("2026-03-01T00:00:00Z"))).await.unwrap();
        create(&db, &buy(acc_c.id, at("2026-05-01T00:00:00Z"))).await.unwrap();

        let rows = accounts_for_instrument(&db, ins.id).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].0, acc_a.id, "tie on count, A more recent -> first");
        assert_eq!(rows[0].1, 2);
        assert_eq!(rows[1].0, acc_b.id, "tie on count, B older -> second");
        assert_eq!(rows[1].1, 2);
        assert_eq!(rows[2].0, acc_c.id, "fewer txns -> last despite newest date");
        assert_eq!(rows[2].1, 1);
        // empty for an instrument with no history
        let ins2 = instruments::create(&db, &instruments::NewInstrument { symbol:"VOO".into(), name:"VOO".into(), instrument_type:"etf".into(), native_currency:"USD".into(), category_id:None, price_source:"manual".into(), decimals:Some(8), note:None }).await.unwrap();
        assert!(accounts_for_instrument(&db, ins2.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn list_recent_orders_newest_first_and_filters() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"IDR".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"BBCA".into(), name:"BCA".into(), instrument_type:"stock_id".into(), native_currency:"IDR".into(), category_id:None, price_source:"manual".into(), decimals:Some(0), note:None }).await.unwrap();
        for (d, q) in [("2026-06-01", "1"), ("2026-06-03", "2"), ("2026-06-02", "3")] {
            create(&db, &NewTransaction {
                account_id: acc.id, instrument_id: ins.id, txn_type: "buy".into(),
                executed_at: chrono::DateTime::parse_from_rfc3339(&format!("{d}T00:00:00Z")).unwrap().with_timezone(&chrono::Utc),
                quantity: q.into(), price_native: "1000".into(), fee_native: None,
                currency: "IDR".into(), fx_to_idr: "1".into(), fx_to_usd: "1".into(),
                note: None, source: None, external_id: None,
            }).await.unwrap();
        }
        let recent = list_recent(&db, 2, None, None).await.unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].executed_at.format("%Y-%m-%d").to_string(), "2026-06-03");
        let by_ins = list_recent(&db, 10, Some(ins.id), None).await.unwrap();
        assert_eq!(by_ins.len(), 3);
        let by_acc = list_recent(&db, 10, None, Some(acc.id)).await.unwrap();
        assert_eq!(by_acc.len(), 3);
        // A non-existent account filters everything out.
        let none = list_recent(&db, 10, None, Some(acc.id + 999)).await.unwrap();
        assert!(none.is_empty());
    }

    #[tokio::test]
    async fn update_changes_quantity_and_price_and_renormalizes_idr() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount {
            name: "Bibit".into(), account_type: "manual".into(), institution: None,
            native_currency: "IDR".into(), note: None,
        }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument {
            symbol: "RD9999".into(), name: "Sucorinvest Money Market".into(),
            instrument_type: "mutual_fund".into(), native_currency: "IDR".into(),
            category_id: None, price_source: "bibit:RD9999".into(),
            decimals: Some(4), note: None,
        }).await.unwrap();
        // seed a USD/IDR rate so IDR normalization can derive fx_to_usd
        crate::repo::prices::upsert_fx(&db, "USD", "IDR", d!(16500), "2026-06-18").await.unwrap();

        // create a value-based (price=1) row — the typical reksadana mis-entry scenario
        let original = create(&db, &NewTransaction {
            account_id: acc.id, instrument_id: ins.id, txn_type: "buy".into(),
            executed_at: chrono::Utc::now(), quantity: "2000000".into(), price_native: "1".into(),
            fee_native: None, currency: "IDR".into(), fx_to_idr: "1".into(), fx_to_usd: "1".into(),
            note: None, source: None, external_id: None,
        }).await.unwrap();

        let patched = update(&db, original.id, &TxnPatch {
            quantity: Some("1236.7898".into()),
            price_native: Some("1617.0896".into()),
            ..Default::default()
        }).await.unwrap();

        assert_eq!(patched.quantity.to_string(), "1236.7898");
        assert_eq!(patched.price_native.to_string(), "1617.0896");
        assert_eq!(patched.fx_to_idr.to_string(), "1"); // IDR identity preserved
    }

    #[tokio::test]
    async fn update_preserves_existing_fx_for_non_idr_currency() {
        // A non-IDR transaction must keep its historical fx rates on edit — the
        // IDR normalization branch must not touch USD (or any non-IDR) rows.
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount {
            name: "IBKR".into(), account_type: "manual".into(), institution: None,
            native_currency: "USD".into(), note: None,
        }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument {
            symbol: "VOO".into(), name: "Vanguard S&P 500".into(),
            instrument_type: "etf".into(), native_currency: "USD".into(),
            category_id: None, price_source: "manual".into(), decimals: Some(8), note: None,
        }).await.unwrap();
        let original = create(&db, &NewTransaction {
            account_id: acc.id, instrument_id: ins.id, txn_type: "buy".into(),
            executed_at: chrono::Utc::now(), quantity: "1".into(), price_native: "500".into(),
            fee_native: None, currency: "USD".into(), fx_to_idr: "16500".into(), fx_to_usd: "1".into(),
            note: None, source: None, external_id: None,
        }).await.unwrap();

        let patched = update(&db, original.id, &TxnPatch {
            quantity: Some("3".into()),
            ..Default::default()
        }).await.unwrap();

        assert_eq!(patched.quantity.to_string(), "3");
        assert_eq!(patched.fx_to_idr.to_string(), "16500"); // historical rate preserved
        assert_eq!(patched.fx_to_usd.to_string(), "1"); // unchanged
    }

    #[tokio::test]
    async fn update_preserves_unpatched_fields() {
        // Fields omitted from the patch must retain their current values, including
        // fee_native (NOT NULL column) and an existing note.
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount {
            name: "A".into(), account_type: "manual".into(), institution: None,
            native_currency: "IDR".into(), note: None,
        }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument {
            symbol: "BBCA".into(), name: "BCA".into(), instrument_type: "stock_id".into(),
            native_currency: "IDR".into(), category_id: None, price_source: "manual".into(),
            decimals: Some(0), note: None,
        }).await.unwrap();
        let original = create(&db, &NewTransaction {
            account_id: acc.id, instrument_id: ins.id, txn_type: "buy".into(),
            executed_at: chrono::Utc::now(), quantity: "100".into(), price_native: "9000".into(),
            fee_native: Some("500".into()), currency: "IDR".into(), fx_to_idr: "1".into(),
            fx_to_usd: "1".into(), note: Some("original note".into()), source: None, external_id: None,
        }).await.unwrap();

        let patched = update(&db, original.id, &TxnPatch {
            quantity: Some("150".into()),
            ..Default::default()
        }).await.unwrap();

        assert_eq!(patched.quantity.to_string(), "150"); // patched
        assert_eq!(patched.price_native.to_string(), "9000"); // preserved
        assert_eq!(patched.fee_native.to_string(), "500"); // preserved (NOT NULL)
        assert_eq!(patched.note, Some("original note".into())); // preserved, not cleared
    }

    #[tokio::test]
    async fn external_id_dedup_via_unique_index() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount{ name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument{ symbol:"ETH".into(), name:"e".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:None, price_source:"manual".into(), decimals:Some(18), note:None }).await.unwrap();
        let mk = || NewTransaction { account_id: acc.id, instrument_id: ins.id, txn_type:"deposit".into(),
            executed_at: Utc::now(), quantity:"1".into(), price_native:"0".into(), fee_native:None,
            currency:"ETH".into(), fx_to_idr:"1".into(), fx_to_usd:"1".into(), note:None,
            source: Some("evm".into()), external_id: Some("0xabc".into()) };
        create(&db, &mk()).await.unwrap();
        assert!(create(&db, &mk()).await.is_err()); // unique index blocks duplicate
        assert_eq!(existing_external_ids(&db, "evm").await.unwrap().len(), 1);
    }
}
