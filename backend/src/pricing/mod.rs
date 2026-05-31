pub mod coingecko;
pub mod fx;
pub mod service;

use rust_decimal::Decimal;

#[derive(Debug, thiserror::Error)]
pub enum PriceError {
    #[error("http error: {0}")]
    Http(String),
    #[error("price not found for {0}")]
    NotFound(String),
    #[error("parse error: {0}")]
    Parse(String),
}

#[derive(Debug, Clone)]
pub struct Quote {
    pub price: Decimal,
    pub currency: String,
}

#[async_trait::async_trait]
pub trait PriceProvider: Send + Sync {
    /// `ext_id` is the provider-specific id, e.g. "bitcoin" for CoinGecko.
    async fn latest(&self, ext_id: &str) -> Result<Quote, PriceError>;
}
