use crate::connectors::{Connector, ConnectorError, ExternalTxn, SyncBatch};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};

pub struct HyperliquidConnector {
    base_url: String,
    token: String,
    client: reqwest::Client,
}

impl HyperliquidConnector {
    pub fn new(base_url: String, token: String) -> Self {
        Self { base_url, token, client: reqwest::Client::new() }
    }
}

#[async_trait]
impl Connector for HyperliquidConnector {
    async fn fetch_new(&self, _cursor: Option<&str>) -> Result<SyncBatch, ConnectorError> {
        let url = format!("{}/flows", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| ConnectorError::Http(e.to_string()))?;
        let json: serde_json::Value =
            resp.json().await.map_err(|e| ConnectorError::Parse(e.to_string()))?;
        Ok(SyncBatch { txns: parse_flows(&json)?, next_cursor: None })
    }
}

/// Map `/flows` rows to deposit/withdrawal [`ExternalTxn`]s (USDC).
///
/// The bot API returns `usdc` as a signed number: negative for withdrawals,
/// positive for deposits. `ExternalTxn.quantity` must be the absolute value
/// (magnitude). Direction comes from the `kind` field, not the sign.
///
/// # TWR external-flow accounting only — no market value
///
/// The [`ExternalTxn`]s produced here are **for TWR (time-weighted return)
/// external-flow accounting only**. They record cash entering or leaving the
/// account so that the TWR calculation can strip out the effect of deposits and
/// withdrawals. They must land on a cash/unpriced path and must **not** be
/// given a market value or turned into a valued holding. The `HL-EQUITY`
/// synthetic instrument already prices the total account equity, which
/// includes all idle USDC collateral. Assigning independent market value to
/// these USDC flows would double-count that collateral in net worth.
///
/// # Errors
/// Returns `ConnectorError::Parse` if the body is not a JSON array.
pub fn parse_flows(body: &serde_json::Value) -> Result<Vec<ExternalTxn>, ConnectorError> {
    let rows = body
        .as_array()
        .ok_or_else(|| ConnectorError::Parse("expected flows array".into()))?;
    let mut out = Vec::new();
    for row in rows {
        let kind = match row.get("kind").and_then(|v| v.as_str()) {
            Some(k @ ("deposit" | "withdrawal")) => k.to_string(),
            _ => continue,
        };
        let quantity = match row.get("usdc") {
            Some(serde_json::Value::Number(n)) => {
                // Apply abs() — usdc may be negative for withdrawals.
                let magnitude = n.as_f64().unwrap_or(0.0).abs();
                rust_decimal::Decimal::try_from(magnitude)
                    .map(|d| d.normalize().to_string())
                    .unwrap_or_else(|_| magnitude.to_string())
            }
            Some(serde_json::Value::String(s)) => {
                // Handle string-encoded values, also apply abs().
                let magnitude = s
                    .parse::<f64>()
                    .map_err(|e| ConnectorError::Parse(format!("usdc string parse failed: {e}")))?
                    .abs();
                rust_decimal::Decimal::try_from(magnitude)
                    .map(|d| d.normalize().to_string())
                    .unwrap_or_else(|_| magnitude.to_string())
            }
            _ => continue,
        };
        let time_ms = row.get("time_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let occurred_at = Utc
            .timestamp_millis_opt(time_ms)
            .single()
            .unwrap_or_else(Utc::now)
            .to_rfc3339();
        let external_id = row
            .get("external_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        out.push(ExternalTxn {
            external_id,
            occurred_at,
            kind,
            symbol: "USDC".into(),
            quantity,
            fee: None,
            currency: "USD".into(),
            // USDC is a stablecoin; price it at 1 so TWR external flows are valued.
            price_native: Some("1".into()),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_deposit_and_withdrawal_flows() {
        let body = serde_json::json!([
            { "external_id": "0xa:deposit", "kind": "deposit", "usdc": 500.0, "time_ms": 1700000000000_i64 },
            { "external_id": "0xb:withdrawal", "kind": "withdrawal", "usdc": 200.0, "time_ms": 1700000100000_i64 }
        ]);
        let out = parse_flows(&body).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].kind, "deposit");
        assert_eq!(out[0].symbol, "USDC");
        assert_eq!(out[0].currency, "USD");
        assert_eq!(out[0].quantity, "500");
        assert_eq!(out[1].kind, "withdrawal");
    }

    #[test]
    fn withdrawal_with_negative_usdc_produces_absolute_quantity() {
        // Bot API returns negative usdc for withdrawals; quantity must be positive magnitude.
        let body = serde_json::json!([
            { "external_id": "0xc:withdrawal", "kind": "withdrawal", "usdc": -200.5, "time_ms": 1700000200000_i64 }
        ]);
        let out = parse_flows(&body).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "withdrawal");
        assert_eq!(out[0].quantity, "200.5");
        assert_eq!(out[0].symbol, "USDC");
        assert_eq!(out[0].currency, "USD");
        assert_eq!(out[0].external_id, "0xc:withdrawal");
    }

    #[test]
    fn deposit_with_positive_usdc_passes_through() {
        let body = serde_json::json!([
            { "external_id": "0xd:deposit", "kind": "deposit", "usdc": 1000.0, "time_ms": 1700000300000_i64 }
        ]);
        let out = parse_flows(&body).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "deposit");
        assert_eq!(out[0].quantity, "1000");
    }

    #[test]
    fn non_array_body_returns_parse_error() {
        let body = serde_json::json!({ "error": "not an array" });
        assert!(parse_flows(&body).is_err());
    }

    #[test]
    fn rows_with_unknown_kind_are_skipped() {
        let body = serde_json::json!([
            { "external_id": "0xe:trade", "kind": "trade", "usdc": 100.0, "time_ms": 1700000400000_i64 },
            { "external_id": "0xf:deposit", "kind": "deposit", "usdc": 50.0, "time_ms": 1700000500000_i64 }
        ]);
        let out = parse_flows(&body).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "deposit");
    }

    #[test]
    fn string_encoded_usdc_deposit_parses_correctly() {
        // Some API responses encode usdc as a JSON string rather than a number.
        let body = serde_json::json!([
            { "external_id": "0xh:deposit", "kind": "deposit", "usdc": "500.0", "time_ms": 1700000000000_i64 }
        ]);
        let out = parse_flows(&body).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "deposit");
        assert_eq!(out[0].symbol, "USDC");
        // Decimal normalize strips trailing zero: "500.0" -> "500"
        assert_eq!(out[0].quantity, "500");
    }

    #[test]
    fn string_encoded_negative_usdc_withdrawal_applies_abs() {
        let body = serde_json::json!([
            { "external_id": "0xi:withdrawal", "kind": "withdrawal", "usdc": "-200.5", "time_ms": 1700000000000_i64 }
        ]);
        let out = parse_flows(&body).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "withdrawal");
        assert_eq!(out[0].quantity, "200.5");
    }

    #[test]
    fn string_encoded_invalid_usdc_returns_parse_error() {
        let body = serde_json::json!([
            { "external_id": "0xj:deposit", "kind": "deposit", "usdc": "not_a_number", "time_ms": 1700000000000_i64 }
        ]);
        let result = parse_flows(&body);
        assert!(result.is_err(), "invalid string usdc must return a parse error");
    }

    #[test]
    fn occurred_at_is_valid_rfc3339() {
        let body = serde_json::json!([
            { "external_id": "0xg:deposit", "kind": "deposit", "usdc": 75.0, "time_ms": 1700000000000_i64 }
        ]);
        let out = parse_flows(&body).unwrap();
        assert_eq!(out.len(), 1);
        // Verify it parses as a valid RFC3339 datetime.
        chrono::DateTime::parse_from_rfc3339(&out[0].occurred_at)
            .expect("occurred_at must be valid RFC3339");
    }
}
