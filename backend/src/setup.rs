//! One-time, idempotent setup for the Hyperliquid equity account.
//!
//! When `HYPERLIQUID_API_URL` is set, the application calls
//! [`ensure_hyperliquid_account`] on startup to provision a synthetic
//! exchange account, a `HL-EQUITY` instrument (priced via `hyperliquid:bot`),
//! and a single quantity-1 opening-balance holding whose market value equals
//! the live Hyperliquid account equity. The function is safe to call on every
//! restart — it is gated on the instrument's existence and exits early if
//! already provisioned.

use crate::db::Db;
use crate::repo::{accounts, instruments, transactions};
use chrono::Utc;

pub const HL_SYMBOL: &str = "HL-EQUITY";
pub const HL_ACCOUNT_NAME: &str = "Hyperliquid";

/// Create the Hyperliquid account, the synthetic `HL-EQUITY` instrument, and a
/// single quantity-1 opening-balance holding. Idempotent: gated on the
/// instrument's existence, so re-running on every startup is safe.
///
/// # Double-counting invariant
///
/// The `HL-EQUITY` instrument price equals the **total account equity** reported
/// by the Hyperliquid bot, which already includes idle USDC collateral. Because
/// of this, a separately-priced `USDC` instrument must **never** be created on
/// the `Hyperliquid` account. USDC deposit and withdrawal flows from the
/// connector are recorded solely as external cash flows for TWR (time-weighted
/// return) accounting — they carry no market value of their own and must never
/// be turned into a valued holding. Creating a priced `USDC` balance alongside
/// `HL-EQUITY` would double-count the same dollars in net worth.
pub async fn ensure_hyperliquid_account(db: &Db) -> anyhow::Result<()> {
    if instruments::find_by_symbol(db, HL_SYMBOL).await?.is_some() {
        return Ok(());
    }
    let account = match accounts::find_by_name(db, HL_ACCOUNT_NAME).await? {
        Some(existing) => existing,
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
        price_source: "hyperliquid:bot".into(),
        decimals: Some(2),
        note: None,
    })
    .await?;
    // Synthetic 1-unit holding; market value comes entirely from the equity
    // price quote provided by the `hyperliquid:bot` pricer × the live USD/IDR fx.
    transactions::create(db, &transactions::NewTransaction {
        account_id: account.id,
        instrument_id: instrument.id,
        txn_type: "opening_balance".into(),
        executed_at: Utc::now(),
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
        ensure_hyperliquid_account(&db).await.unwrap();
        ensure_hyperliquid_account(&db).await.unwrap(); // no-op second time

        let instrument = instruments::find_by_symbol(&db, HL_SYMBOL).await.unwrap().expect("instrument");
        assert_eq!(instrument.price_source, "hyperliquid:bot");
        assert_eq!(instrument.native_currency, "USD");
        let account = accounts::find_by_name(&db, HL_ACCOUNT_NAME).await.unwrap().expect("account");
        assert_eq!(account.account_type, "exchange");
        let all_instruments = instruments::list(&db).await.unwrap();
        assert_eq!(all_instruments.iter().filter(|i| i.symbol == HL_SYMBOL).count(), 1);
    }
}
