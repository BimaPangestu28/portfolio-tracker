//! Daily movers: each held position's day-over-day price change applied to
//! the held quantity, in IDR — powers the dashboard's "Pergerakan Hari Ini".

use crate::db::Db;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct Mover {
    pub instrument_id: i64,
    pub symbol: String,
    pub name: String,
    /// Day change of the whole position, IDR.
    #[serde(with = "rust_decimal::serde::str")]
    pub delta_idr: Decimal,
    /// Day change of the price itself, percent.
    pub delta_pct: f64,
    #[serde(with = "rust_decimal::serde::str")]
    pub value_idr: Decimal,
}

/// Day change for one position: price delta × quantity × FX-to-IDR, plus the
/// price's own percent move.
pub fn mover_delta(latest: Decimal, prev: Decimal, qty: Decimal, fx_to_idr: Decimal) -> (Decimal, f64) {
    let delta_idr = (latest - prev) * qty * fx_to_idr;
    let pct = if prev.is_zero() {
        0.0
    } else {
        ((latest - prev) / prev * Decimal::from(100)).to_f64().unwrap_or(0.0)
    };
    (delta_idr, pct)
}

/// Top `limit` movers by absolute IDR impact. Positions without two days of
/// quotes (fresh instruments, manual prices) are skipped.
pub async fn daily_movers(db: &Db, limit: usize) -> anyhow::Result<Vec<Mover>> {
    let summary = crate::service::portfolio::build_summary(db).await?;
    let instruments = crate::repo::instruments::list(db).await?;
    let instrument_by_id: HashMap<i64, _> =
        instruments.into_iter().map(|row| (row.id, row)).collect();
    let usd_idr = crate::repo::prices::latest_fx(db, "USD", "IDR")
        .await?
        .unwrap_or(Decimal::ONE);

    let mut movers = Vec::new();
    for position in &summary.positions {
        if position.quantity.is_zero() {
            continue;
        }
        let Some(instrument) = instrument_by_id.get(&position.instrument_id) else { continue };
        let quotes = crate::repo::prices::last_two(db, position.instrument_id).await?;
        if quotes.len() < 2 {
            continue;
        }
        let fx = if instrument.native_currency.eq_ignore_ascii_case("USD") {
            usd_idr
        } else {
            Decimal::ONE
        };
        let (delta_idr, delta_pct) =
            mover_delta(quotes[0].price, quotes[1].price, position.quantity, fx);
        movers.push(Mover {
            instrument_id: position.instrument_id,
            symbol: instrument.symbol.clone(),
            name: instrument.name.clone(),
            delta_idr,
            delta_pct,
            value_idr: position.market_value_idr,
        });
    }
    movers.sort_by_key(|m| std::cmp::Reverse(m.delta_idr.abs()));
    movers.truncate(limit);
    Ok(movers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn delta_applies_quantity_and_fx() {
        // TLKM: 2870 -> 2900, 700 shares, IDR (fx 1)
        let (delta, pct) = mover_delta(dec!(2900), dec!(2870), dec!(700), Decimal::ONE);
        assert_eq!(delta, dec!(21000));
        assert!((pct - 1.045).abs() < 0.01, "{pct}");
    }

    #[test]
    fn usd_positions_convert_to_idr() {
        // VOO: 500 -> 510 USD, 2 shares, 16300 IDR/USD
        let (delta, _) = mover_delta(dec!(510), dec!(500), dec!(2), dec!(16300));
        assert_eq!(delta, dec!(326000));
    }

    #[test]
    fn zero_previous_price_gives_zero_pct() {
        let (_, pct) = mover_delta(dec!(100), dec!(0), dec!(1), Decimal::ONE);
        assert_eq!(pct, 0.0);
    }
}
