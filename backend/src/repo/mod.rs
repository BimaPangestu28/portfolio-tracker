pub mod accounts;
pub mod events;
pub mod google_integration;
pub mod upwork_integration;
pub mod upwork_project_link;
pub mod proactive_log;
pub mod cashflow;
pub mod cashflow_categories;
pub mod categories;
pub mod chat;
pub mod connectors;
pub mod goals;
pub mod inbox;
pub mod instruments;
pub mod transactions;
pub mod price_alerts;
pub mod prices;
pub mod snapshots;
pub mod review_items;
pub mod telegram_link;
pub mod todos;
pub mod reminders;
pub mod clients;
pub mod invoices;
pub mod news;
pub mod cs;

use rust_decimal::Decimal;
use std::str::FromStr;

/// Parse a TEXT decimal column into Decimal, mapping errors to anyhow.
pub fn dec(s: &str) -> anyhow::Result<Decimal> {
    Decimal::from_str(s).map_err(|e| anyhow::anyhow!("bad decimal '{s}': {e}"))
}
