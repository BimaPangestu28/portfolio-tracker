//! Upwork earnings integration: OAuth2, a mockable transaction client, a pure
//! reconciler, and the sync engine. Earnings land in the `cashflow` ledger as
//! income; the portfolio domain is never touched. Mirrors the `google` module.

pub mod client;
pub mod crypto;
pub mod engine;
pub mod jobs;
pub mod oauth;
pub mod sync;

/// One Upwork financial transaction, decoded from the GraphQL API or a fixture.
#[derive(Debug, Clone, PartialEq)]
pub struct UpworkTransaction {
    /// Upwork transaction reference — the idempotency key.
    pub external_id: String,
    /// Date the transaction occurred (YYYY-MM-DD or rfc3339).
    pub date: String,
    /// Raw Upwork type/description, used to classify earning vs fee/withdrawal.
    pub kind: String,
    /// Money as a string (never parsed for storage).
    pub amount: String,
    /// Currency code, e.g. "USD".
    pub currency: String,
    /// Contract / project name, if any.
    pub contract: Option<String>,
}
