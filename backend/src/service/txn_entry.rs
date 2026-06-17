//! Resolve a trade's (quantity, price_native) from whatever value fields the
//! caller has — units+NAV, amount+NAV, amount+units, or amount-only. Shared by
//! the OCR confirm path and the chat transaction tools so both record reksadana
//! buys as real units (quantity = amount/NAV) instead of quantity = rupiah.

use crate::db::Db;
use crate::repo::instruments::InstrumentRow;
use crate::repo::{prices, transactions};
use rust_decimal::Decimal;

/// Why a trade could not be resolved into concrete units and price.
pub enum ResolveError {
    /// A fund trade carried only a rupiah amount and no NAV/units could be
    /// derived — the caller must ask the user for NAV or unit count.
    NeedNavOrUnits,
    Other(anyhow::Error),
}

impl From<anyhow::Error> for ResolveError {
    fn from(e: anyhow::Error) -> Self {
        ResolveError::Other(e)
    }
}

fn clean(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

/// Append a parenthetical note, joining onto any existing note.
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

/// Resolve a fund amount-only trade. For a bibit-sourced fund with a stored NAV
/// quote, derive real units (amount / NAV at 4 dp). Otherwise either fall back
/// to quantity = amount at price 1 (OCR fresh-buy) or signal NeedNavOrUnits
/// (manual entry). Reads only the quote table; never touches the network.
async fn amount_only(
    db: &Db,
    ins: &InstrumentRow,
    amount: &str,
    allow_price_one_fallback: bool,
    note: &mut Option<String>,
) -> Result<(String, String), ResolveError> {
    let amount_dec = crate::repo::dec(amount)?;
    if ins.price_source.starts_with("bibit:") {
        // Once an instrument has value-based (price = 1) rows, stay on that
        // convention — mixing NAV-derived units with rupiah-as-units rows makes
        // the position unreconcilable. Edit the legacy rows to real units to
        // unlock derivation.
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
        if !allow_price_one_fallback {
            return Err(ResolveError::NeedNavOrUnits);
        }
        append_note(note, "NAV belum tersedia; dicatat nominal di harga 1");
    }
    Ok((amount_dec.normalize().to_string(), "1".to_string()))
}

/// Resolve (quantity, price_native). See module docs for the value-field matrix.
pub async fn resolve_qty_price(
    db: &Db,
    ins: &InstrumentRow,
    entry_type: &str,
    quantity: Option<&str>,
    price_native: Option<&str>,
    amount_native: Option<&str>,
    allow_price_one_fallback: bool,
    note: &mut Option<String>,
) -> Result<(String, String), ResolveError> {
    let q = clean(quantity);
    let p = clean(price_native);
    let a = clean(amount_native);

    // units + price: use verbatim.
    if let (Some(q), Some(p)) = (q, p) {
        return Ok((q.to_string(), p.to_string()));
    }
    // amount + price (NAV): qty = amount / price (4 dp, bibit unit precision).
    if let (Some(a), Some(p)) = (a, p) {
        let price = crate::repo::dec(p)?;
        let qty = (crate::repo::dec(a)? / price).round_dp(4);
        return Ok((qty.normalize().to_string(), price.normalize().to_string()));
    }
    // amount + units: price = amount / units.
    if let (Some(a), Some(q)) = (a, q) {
        let units = crate::repo::dec(q)?;
        let price = crate::repo::dec(a)? / units;
        return Ok((units.normalize().to_string(), price.normalize().to_string()));
    }
    // amount only: fund-aware derivation (buy/sell only).
    if let Some(a) = a {
        if matches!(entry_type, "buy" | "sell") {
            return amount_only(db, ins, a, allow_price_one_fallback, note).await;
        }
    }
    // quantity only (e.g. dividend in units): price defaults to 0.
    if let Some(q) = q {
        return Ok((q.to_string(), "0".to_string()));
    }
    Err(ResolveError::Other(anyhow::anyhow!(
        "butuh quantity+price atau amount untuk entry {entry_type}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::instruments::{self, NewInstrument};

    async fn fund(db: &Db) -> InstrumentRow {
        instruments::create(db, &NewInstrument {
            symbol: "MJR".into(), name: "Majoris Pasar Uang".into(), instrument_type: "fund".into(),
            native_currency: "IDR".into(), category_id: None,
            price_source: "bibit:MJR02".into(), decimals: Some(4), note: None,
        }).await.unwrap();
        instruments::list(db).await.unwrap().pop().unwrap()
    }

    #[tokio::test]
    async fn units_and_nav_pass_through() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = fund(&db).await;
        let mut note = None;
        let (q, p) = resolve_qty_price(&db, &ins, "buy", Some("1236.7898"), Some("1617.0896"), None, false, &mut note).await.ok().unwrap();
        assert_eq!(q, "1236.7898");
        assert_eq!(p, "1617.0896");
    }

    #[tokio::test]
    async fn amount_and_nav_derives_units() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = fund(&db).await;
        let mut note = None;
        let (q, p) = resolve_qty_price(&db, &ins, "buy", None, Some("1617.0896"), Some("2000000"), false, &mut note).await.ok().unwrap();
        assert_eq!(q, "1236.7898"); // 2000000 / 1617.0896 = 1236.78984..., 4dp
        assert_eq!(p, "1617.0896");
    }

    #[tokio::test]
    async fn amount_and_units_derives_price() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = fund(&db).await;
        let mut note = None;
        let (q, p) = resolve_qty_price(&db, &ins, "buy", Some("12367.8985"), None, Some("20000000"), false, &mut note).await.ok().unwrap();
        assert_eq!(q, "12367.8985");
        assert_eq!(p, "1617.0895969109060848130343243"); // 20000000 / 12367.8985
    }

    #[tokio::test]
    async fn fund_amount_only_without_nav_asks_when_no_fallback() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = fund(&db).await;
        let mut note = None;
        let err = resolve_qty_price(&db, &ins, "buy", None, None, Some("2000000"), false, &mut note).await.err().unwrap();
        assert!(matches!(err, ResolveError::NeedNavOrUnits));
    }

    #[tokio::test]
    async fn fund_amount_only_without_nav_falls_back_when_allowed() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = fund(&db).await;
        let mut note = None;
        let (q, p) = resolve_qty_price(&db, &ins, "buy", None, None, Some("2000000"), true, &mut note).await.ok().unwrap();
        assert_eq!(q, "2000000");
        assert_eq!(p, "1");
        assert!(note.unwrap().contains("NAV belum tersedia"));
    }
}
