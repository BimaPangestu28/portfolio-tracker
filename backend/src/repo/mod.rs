pub mod accounts;
pub mod cashflow;
pub mod cashflow_categories;
pub mod categories;
pub mod instruments;
pub mod transactions;
pub mod prices;
pub mod snapshots;
pub mod review_items;

use rust_decimal::Decimal;
use std::str::FromStr;

/// Parse a TEXT decimal column into Decimal, mapping errors to anyhow.
pub fn dec(s: &str) -> anyhow::Result<Decimal> {
    Decimal::from_str(s).map_err(|e| anyhow::anyhow!("bad decimal '{s}': {e}"))
}
