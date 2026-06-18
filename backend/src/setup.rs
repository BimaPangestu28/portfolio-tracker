//! One-time, idempotent setup for the Hyperliquid equity account.

use crate::db::Db;
use crate::repo::{accounts, instruments, transactions};

pub const HL_SYMBOL: &str = "HL-EQUITY";
pub const HL_ACCOUNT_NAME: &str = "Hyperliquid";

/// Create the Hyperliquid account, the synthetic `HL-EQUITY` instrument, and a
/// single quantity-1 opening-balance holding. Idempotent: gated on the
/// instrument's existence, so re-running on every startup is safe.
pub async fn ensure_hyperliquid_account(db: &Db, wallet: &str) -> anyhow::Result<()> {
    if instruments::find_by_symbol(db, HL_SYMBOL).await?.is_some() {
        return Ok(());
    }
    let account = match accounts::find_by_name(db, HL_ACCOUNT_NAME).await? {
        Some(a) => a,
        None => {
            accounts::create(db, &accounts::NewAccount {
                name: HL_ACCOUNT_NAME.into(),
                account_type: "exchange".into(),
                institution: Some("Hyperliquid".into()),
                native_currency: "USD".into(),
                note: Some("Auto-created for Hyperliquid equity tracking".into()),
            })
            .await?
        }
    };
    let instrument = instruments::create(db, &instruments::NewInstrument {
        symbol: HL_SYMBOL.into(),
        name: "Hyperliquid Account Equity".into(),
        instrument_type: "other".into(),
        native_currency: "USD".into(),
        category_id: None,
        price_source: format!("hyperliquid:{wallet}"),
        decimals: Some(2),
        note: None,
    })
    .await?;
    // Synthetic 1-unit holding. price_native = 0, so cost basis is 0; market
    // value comes entirely from the equity price quote x the live USD/IDR fx.
    transactions::create(db, &transactions::NewTransaction {
        account_id: account.id,
        instrument_id: instrument.id,
        txn_type: "opening_balance".into(),
        executed_at: chrono::Utc::now(),
        quantity: "1".into(),
        price_native: "0".into(),
        fee_native: None,
        currency: "USD".into(),
        fx_to_idr: "1".into(),
        fx_to_usd: "1".into(),
        note: Some("Synthetic 1-unit holding; value = account equity".into()),
        source: Some("hyperliquid-setup".into()),
        external_id: Some("hl-equity-opening".into()),
    })
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ensure_is_idempotent_and_creates_synthetic_holding() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        ensure_hyperliquid_account(&db, "0xWALLET").await.unwrap();
        // Second call is a no-op (no duplicate instrument).
        ensure_hyperliquid_account(&db, "0xWALLET").await.unwrap();

        let ins = instruments::find_by_symbol(&db, HL_SYMBOL).await.unwrap().expect("instrument");
        assert_eq!(ins.price_source, "hyperliquid:0xWALLET");
        assert_eq!(ins.native_currency, "USD");

        let acct = accounts::find_by_name(&db, HL_ACCOUNT_NAME).await.unwrap().expect("account");
        assert_eq!(acct.account_type, "exchange");

        let all = instruments::list(&db).await.unwrap();
        assert_eq!(all.iter().filter(|i| i.symbol == HL_SYMBOL).count(), 1);
    }
}
