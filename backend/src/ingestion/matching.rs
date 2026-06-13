use crate::db::Db;

pub async fn suggest_instrument(db: &Db, symbol: &str) -> anyhow::Result<Option<i64>> {
    let row = sqlx::query_as::<_, (i64,)>("SELECT id FROM instrument WHERE LOWER(symbol) = LOWER(?) LIMIT 1")
        .bind(symbol).fetch_optional(db).await?;
    Ok(row.map(|(id,)| id))
}

/// Suggest an instrument for an extracted entry: exact symbol match first, then
/// exact name match. Mutual funds (e.g. Bibit) carry a fund name but no ticker,
/// so the name fallback is what makes their suggestions work at all.
pub async fn suggest_instrument_for_entry(db: &Db, symbol: Option<&str>, name: Option<&str>) -> anyhow::Result<Option<i64>> {
    if let Some(s) = symbol {
        if let Some(id) = suggest_instrument(db, s).await? {
            return Ok(Some(id));
        }
    }
    if let Some(n) = name {
        let row = sqlx::query_as::<_, (i64,)>("SELECT id FROM instrument WHERE LOWER(name) = LOWER(?) LIMIT 1")
            .bind(n).fetch_optional(db).await?;
        return Ok(row.map(|(id,)| id));
    }
    Ok(None)
}

pub async fn suggest_account(db: &Db, name: &str) -> anyhow::Result<Option<i64>> {
    let row = sqlx::query_as::<_, (i64,)>("SELECT id FROM account WHERE LOWER(name) = LOWER(?) LIMIT 1")
        .bind(name).fetch_optional(db).await?;
    Ok(row.map(|(id,)| id))
}

/// Resolve the account for an extracted entry, checking the DB before giving up:
/// exact name match on the hint, then the instrument's transaction history
/// (most-frequent account), then a single unambiguous fuzzy name match. Returns
/// None only when nothing in the DB points at an account — the caller then asks
/// the owner in chat. Never silently writes, so a best-guess default is safe.
pub async fn resolve_account(
    db: &Db,
    account_hint: Option<&str>,
    instrument_id: Option<i64>,
) -> anyhow::Result<Option<i64>> {
    // 1. Exact name match on the hint — the strongest, most reliable signal.
    if let Some(hint) = account_hint {
        if let Some(id) = suggest_account(db, hint).await? {
            return Ok(Some(id));
        }
    }
    // 2. Infer from the instrument's transaction history.
    if let Some(instrument_id) = instrument_id {
        let history = crate::repo::transactions::accounts_for_instrument(db, instrument_id).await?;
        if let Some((account_id, _, _)) = history.first() {
            return Ok(Some(*account_id));
        }
    }
    // 3. Single unambiguous fuzzy name match on the hint.
    if let Some(hint) = account_hint {
        if let Some(id) = fuzzy_account(db, hint).await? {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

/// Case-insensitive containment match (either direction) on account name.
/// Returns Some only when EXACTLY ONE account matches; zero or many -> None so
/// the assistant asks rather than guessing wrong.
async fn fuzzy_account(db: &Db, hint: &str) -> anyhow::Result<Option<i64>> {
    let needle = hint.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(None);
    }
    let rows = sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM account \
         WHERE INSTR(LOWER(name), ?) > 0 OR INSTR(?, LOWER(name)) > 0",
    )
    .bind(&needle)
    .bind(&needle)
    .fetch_all(db)
    .await?;
    if rows.len() == 1 {
        Ok(Some(rows[0].0))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{accounts, instruments};

    #[tokio::test]
    async fn suggests_instrument_by_symbol_case_insensitive() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"BTC".into(), name:"Bitcoin".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:None, price_source:"manual".into(), decimals:Some(8), note:None }).await.unwrap();
        assert_eq!(suggest_instrument(&db, "btc").await.unwrap(), Some(ins.id));
        assert_eq!(suggest_instrument(&db, "ETH").await.unwrap(), None);
    }

    #[tokio::test]
    async fn suggests_account_by_name_case_insensitive() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let a = accounts::create(&db, &accounts::NewAccount { name:"Binance".into(), account_type:"exchange".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        assert_eq!(suggest_account(&db, "binance").await.unwrap(), Some(a.id));
        assert_eq!(suggest_account(&db, "Indodax").await.unwrap(), None);
    }

    #[tokio::test]
    async fn suggest_instrument_for_entry_falls_back_to_name() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument {
            symbol:"SBF".into(), name:"Sucorinvest Bond Fund".into(),
            instrument_type:"mutual_fund".into(), native_currency:"IDR".into(),
            category_id:None, price_source:"manual".into(), decimals:Some(4), note:None,
        }).await.unwrap();
        // mutual funds: no symbol extracted -> exact name match, case-insensitive
        assert_eq!(suggest_instrument_for_entry(&db, None, Some("sucorinvest bond fund")).await.unwrap(), Some(ins.id));
        // symbol match still takes precedence
        assert_eq!(suggest_instrument_for_entry(&db, Some("sbf"), None).await.unwrap(), Some(ins.id));
        // unmatched symbol falls through to the name
        assert_eq!(suggest_instrument_for_entry(&db, Some("XXXX"), Some("Sucorinvest Bond Fund")).await.unwrap(), Some(ins.id));
        // nothing matches
        assert_eq!(suggest_instrument_for_entry(&db, Some("XXXX"), Some("Unknown Fund")).await.unwrap(), None);
        assert_eq!(suggest_instrument_for_entry(&db, None, None).await.unwrap(), None);
    }

    use crate::repo::transactions::{self, NewTransaction};

    async fn buy(db: &Db, account_id: i64, instrument_id: i64) {
        transactions::create(db, &NewTransaction {
            account_id, instrument_id, txn_type:"buy".into(),
            executed_at: chrono::Utc::now(), quantity:"1".into(), price_native:"100".into(),
            fee_native:None, currency:"USD".into(), fx_to_idr:"16000".into(), fx_to_usd:"1".into(),
            note:None, source:None, external_id:None,
        }).await.unwrap();
    }

    async fn mk_account(db: &Db, name: &str) -> i64 {
        accounts::create(db, &accounts::NewAccount { name:name.into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap().id
    }

    async fn mk_instrument(db: &Db, symbol: &str) -> i64 {
        instruments::create(db, &instruments::NewInstrument { symbol:symbol.into(), name:symbol.into(), instrument_type:"etf".into(), native_currency:"USD".into(), category_id:None, price_source:"manual".into(), decimals:Some(8), note:None }).await.unwrap().id
    }

    #[tokio::test]
    async fn resolve_account_prefers_exact_name_over_history() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ibkr = mk_account(&db, "IBKR").await;
        let pluang = mk_account(&db, "Pluang").await;
        let qqqm = mk_instrument(&db, "QQQM").await;
        buy(&db, pluang, qqqm).await; // history points at Pluang
        // exact hint "ibkr" must win over the history account
        assert_eq!(resolve_account(&db, Some("ibkr"), Some(qqqm)).await.unwrap(), Some(ibkr));
    }

    #[tokio::test]
    async fn resolve_account_infers_from_single_history() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ibkr = mk_account(&db, "IBKR").await;
        let qqqm = mk_instrument(&db, "QQQM").await;
        buy(&db, ibkr, qqqm).await;
        // no hint at all -> inferred from the instrument's history
        assert_eq!(resolve_account(&db, None, Some(qqqm)).await.unwrap(), Some(ibkr));
    }

    #[tokio::test]
    async fn resolve_account_picks_most_frequent_when_history_spans_accounts() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ibkr = mk_account(&db, "IBKR").await;
        let pluang = mk_account(&db, "Pluang").await;
        let qqqm = mk_instrument(&db, "QQQM").await;
        buy(&db, ibkr, qqqm).await;
        buy(&db, ibkr, qqqm).await;
        buy(&db, pluang, qqqm).await;
        assert_eq!(resolve_account(&db, None, Some(qqqm)).await.unwrap(), Some(ibkr));
    }

    #[tokio::test]
    async fn resolve_account_history_beats_fuzzy_name() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ibkr = mk_account(&db, "IBKR").await;
        let _other = mk_account(&db, "Pluang Premium").await;
        let qqqm = mk_instrument(&db, "QQQM").await;
        buy(&db, ibkr, qqqm).await;
        // "pluang" would fuzzy-match "Pluang Premium", but history (IBKR) is checked first
        assert_eq!(resolve_account(&db, Some("pluang"), Some(qqqm)).await.unwrap(), Some(ibkr));
    }

    #[tokio::test]
    async fn resolve_account_single_fuzzy_match() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ibkr = mk_account(&db, "IBKR Pro").await;
        let qqqm = mk_instrument(&db, "QQQM").await; // no history
        // hint "ibkr" is contained in "IBKR Pro" (case-insensitive) -> single match
        assert_eq!(resolve_account(&db, Some("ibkr"), Some(qqqm)).await.unwrap(), Some(ibkr));
    }

    #[tokio::test]
    async fn resolve_account_ambiguous_fuzzy_is_none() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let _a = mk_account(&db, "Bank Jago").await;
        let _b = mk_account(&db, "Bank BCA").await;
        let qqqm = mk_instrument(&db, "QQQM").await; // no history
        assert_eq!(resolve_account(&db, Some("bank"), Some(qqqm)).await.unwrap(), None);
    }

    #[tokio::test]
    async fn resolve_account_none_when_nothing_matches() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let _a = mk_account(&db, "IBKR").await;
        let qqqm = mk_instrument(&db, "QQQM").await; // no history
        assert_eq!(resolve_account(&db, Some("nonexistent"), Some(qqqm)).await.unwrap(), None);
        assert_eq!(resolve_account(&db, None, Some(qqqm)).await.unwrap(), None);
        assert_eq!(resolve_account(&db, None, None).await.unwrap(), None);
    }
}
