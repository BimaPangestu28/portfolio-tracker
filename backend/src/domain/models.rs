use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

macro_rules! str_enum {
    ($name:ident { $($variant:ident => $s:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $name { $($variant),+ }
        impl $name {
            pub fn as_str(&self) -> &'static str { match self { $(Self::$variant => $s),+ } }
        }
        impl FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, String> {
                match s { $($s => Ok(Self::$variant),)+ other => Err(format!("invalid {}: {}", stringify!($name), other)) }
            }
        }
    };
}

str_enum!(AccountType {
    Exchange => "exchange",
    Broker => "broker",
    Bank => "bank",
    Wallet => "wallet",
    Manual => "manual"
});

str_enum!(InstrumentType {
    Crypto => "crypto",
    StockId => "stock_id",
    StockUs => "stock_us",
    Etf => "etf",
    MutualFund => "mutual_fund",
    Cash => "cash",
    Bond => "bond",
    Gold => "gold",
    Other => "other"
});

str_enum!(TxnType {
    Buy => "buy",
    Sell => "sell",
    Dividend => "dividend",
    Interest => "interest",
    Fee => "fee",
    Deposit => "deposit",
    Withdrawal => "withdrawal",
    OpeningBalance => "opening_balance"
});

#[derive(Debug, Clone, Serialize)]
pub struct Transaction {
    pub id: i64,
    pub account_id: i64,
    pub instrument_id: i64,
    pub txn_type: TxnType,
    pub executed_at: DateTime<Utc>,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub price_native: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub fee_native: Decimal,
    pub currency: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub fx_to_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub fx_to_usd: Decimal,
    pub note: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txn_type_round_trip() {
        for t in [
            "buy",
            "sell",
            "dividend",
            "interest",
            "fee",
            "deposit",
            "withdrawal",
            "opening_balance",
        ] {
            assert_eq!(TxnType::from_str(t).unwrap().as_str(), t);
        }
    }

    #[test]
    fn invalid_txn_type_errors() {
        assert!(TxnType::from_str("nope").is_err());
    }
}
