//! Pull Hyperliquid perp positions/trades from the bot API into local tables.

use crate::db::Db;
use crate::repo::hl::{insert_trade_if_new, replace_positions, HlPosition, HlTrade};
use chrono::{TimeZone, Utc};

/// Trim a JSON number to a plain decimal string ("2100.0" -> "2100").
fn num_str(v: &serde_json::Value, key: &str) -> String {
    match v.get(key) {
        Some(serde_json::Value::Number(n)) => {
            let s = n.to_string();
            if let Ok(d) = rust_decimal::Decimal::from_str_exact(&s) {
                return d.normalize().to_string();
            }
            s
        }
        Some(serde_json::Value::String(s)) => s.clone(),
        _ => "0".into(),
    }
}

fn ms_to_rfc3339(v: &serde_json::Value, key: &str) -> String {
    let ms = v.get(key).and_then(|x| x.as_i64()).unwrap_or(0);
    Utc.timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

pub fn parse_positions(body: &serde_json::Value, now: &str) -> Vec<HlPosition> {
    body.as_array()
        .map(|rows| {
            rows.iter()
                .map(|r| HlPosition {
                    coin: r
                        .get("coin")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    direction: r
                        .get("direction")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    size: num_str(r, "size"),
                    entry_px: num_str(r, "entry_px"),
                    mark_px: num_str(r, "mark_px"),
                    unrealized_pnl: num_str(r, "unrealized_pnl"),
                    leverage: num_str(r, "leverage"),
                    notional: num_str(r, "notional"),
                    updated_at: now.to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_trades(body: &serde_json::Value) -> Vec<HlTrade> {
    body.as_array()
        .map(|rows| {
            rows.iter()
                .map(|r| HlTrade {
                    external_id: r
                        .get("external_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    coin: r
                        .get("coin")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    direction: r
                        .get("direction")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    size: num_str(r, "size"),
                    entry_px: num_str(r, "entry_px"),
                    exit_px: num_str(r, "exit_px"),
                    realized_pnl: num_str(r, "realized_pnl"),
                    fee: num_str(r, "fee"),
                    opened_at: ms_to_rfc3339(r, "opened_at_ms"),
                    closed_at: ms_to_rfc3339(r, "closed_at_ms"),
                    leverage: r.get("leverage").and_then(|v| v.as_i64()),
                    confidence: r.get("confidence").and_then(|v| v.as_i64()),
                    timeframe: r
                        .get("timeframe")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    profile: r
                        .get("profile")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Pull positions + trades from the bot API and persist to local tables.
/// No-op (returns Ok) when `HYPERLIQUID_API_URL` or `HYPERLIQUID_API_TOKEN` are unset.
pub async fn run(db: &Db) -> anyhow::Result<()> {
    let (base, token) = match (
        std::env::var("HYPERLIQUID_API_URL")
            .ok()
            .filter(|s| !s.is_empty()),
        std::env::var("HYPERLIQUID_API_TOKEN")
            .ok()
            .filter(|s| !s.is_empty()),
    ) {
        (Some(b), Some(t)) => (b, t),
        _ => return Ok(()),
    };
    let base = base.trim_end_matches('/');
    let client = reqwest::Client::new();
    let now = Utc::now().to_rfc3339();

    let positions: serde_json::Value = client
        .get(format!("{base}/positions"))
        .bearer_auth(&token)
        .send()
        .await?
        .json()
        .await?;
    replace_positions(db, &parse_positions(&positions, &now)).await?;

    let trades: serde_json::Value = client
        .get(format!("{base}/trades"))
        .bearer_auth(&token)
        .send()
        .await?
        .json()
        .await?;
    for trade in parse_trades(&trades) {
        insert_trade_if_new(db, &trade).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_positions_response() {
        let body = serde_json::json!([
            { "coin": "ETH", "direction": "long", "size": 1.0, "entry_px": 2000.0,
              "mark_px": 2100.0, "unrealized_pnl": 100.0, "leverage": 5.0, "notional": 2100.0 }
        ]);
        let rows = parse_positions(&body, "2026-06-18T00:00:00Z");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].coin, "ETH");
        assert_eq!(rows[0].mark_px, "2100");
        assert_eq!(rows[0].updated_at, "2026-06-18T00:00:00Z");
    }

    #[test]
    fn parses_trades_response_with_metadata() {
        let body = serde_json::json!([
            { "external_id": "ETH:1:2000", "coin": "ETH", "direction": "long",
              "size": 1.0, "entry_px": 2000.0, "exit_px": 2100.0, "realized_pnl": 100.0,
              "fee": 2.0, "opened_at_ms": 1700000000000_i64, "closed_at_ms": 1700000100000_i64,
              "confidence": 80, "timeframe": "4h", "profile": "moderate", "leverage": 5 }
        ]);
        let rows = parse_trades(&body);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].external_id, "ETH:1:2000");
        assert_eq!(rows[0].realized_pnl, "100");
        assert_eq!(rows[0].confidence, Some(80));
        assert!(rows[0].closed_at.starts_with("2023-11-14")); // ms epoch → rfc3339
    }
}
