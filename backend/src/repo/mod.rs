pub mod accounts;
pub mod cashflow;
pub mod cashflow_categories;
pub mod categories;
pub mod chat;
pub mod connectors;
pub mod goals;
pub mod instruments;
pub mod transactions;
pub mod prices;
pub mod snapshots;
pub mod review_items;
pub mod telegram_link;
pub mod todos;

use rust_decimal::Decimal;
use std::str::FromStr;

/// Parse a TEXT decimal column into Decimal, mapping errors to anyhow.
pub fn dec(s: &str) -> anyhow::Result<Decimal> {
    Decimal::from_str(s).map_err(|e| anyhow::anyhow!("bad decimal '{s}': {e}"))
}
