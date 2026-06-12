//! Event-driven financial alerts, evaluated from already-stored data.
//! Pure helpers produce (dedup_key, message) pairs; the tick claims and sends.

use crate::db::Db;
use crate::service::chat::group_id;
use crate::service::movers::Mover;
use rust_decimal::Decimal;

/// One alert ready to claim-and-send.
#[derive(Debug)]
pub struct Alert {
    pub dedup_key: String,
    pub message: String,
}

/// Format a percent the Indonesian way (comma decimal), with sign.
fn fmt_pct(pct: f64) -> String {
    format!("{pct:+.1}%").replace('.', ",")
}

/// Big daily movers at or beyond the threshold — one alert per instrument per day.
pub fn mover_alerts(movers: &[Mover], threshold_pct: f64, today_wib: &str) -> Vec<Alert> {
    movers
        .iter()
        .filter(|m| m.delta_pct.abs() >= threshold_pct)
        .map(|m| {
            let arrow = if m.delta_pct >= 0.0 { "📈" } else { "📉" };
            let sign = if m.delta_idr.is_sign_negative() { "-" } else { "+" };
            Alert {
                dedup_key: format!("mover:{}:{today_wib}", m.symbol),
                message: format!(
                    "{arrow} {} {} hari ini ({}Rp {})",
                    m.symbol,
                    fmt_pct(m.delta_pct),
                    sign,
                    group_id(&m.delta_idr.abs()),
                ),
            }
        })
        .collect()
}

/// Milestone values crossed moving upward from prev to curr (inclusive curr).
pub fn milestones_crossed(prev_idr: i64, curr_idr: i64, step: i64) -> Vec<i64> {
    if step <= 0 || curr_idr <= prev_idr {
        return Vec::new();
    }
    let first = (prev_idr / step + 1) * step;
    let mut crossed = Vec::new();
    let mut milestone = first;
    while milestone <= curr_idr {
        crossed.push(milestone);
        milestone += step;
    }
    crossed
}

pub fn milestone_alert(milestone_idr: i64) -> Alert {
    Alert {
        dedup_key: format!("milestone:{milestone_idr}"),
        message: format!("🎉 Net worth melewati Rp {}!", group_id(&Decimal::from(milestone_idr))),
    }
}

/// One alert per day while any pending review item is older than 24h.
pub async fn review_backlog_alert(db: &Db, today_wib: &str) -> anyhow::Result<Option<Alert>> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
    let pending = crate::repo::review_items::list_by_status(db, "pending").await?;
    // Both sides come from chrono's to_rfc3339() (fixed-width +00:00), so
    // lexicographic comparison is chronological comparison.
    let old = pending.iter().filter(|item| item.created_at <= cutoff).count();
    if old == 0 {
        return Ok(None);
    }
    Ok(Some(Alert {
        dedup_key: format!("review-backlog:{today_wib}"),
        message: format!(
            "🧾 {old} transaksi menunggu review lebih dari 24 jam — konfirmasi lewat tombol di chat atau web UI → Data."
        ),
    }))
}

/// Stale-price alerts: held positions whose price is stale and whose source
/// is not manual (manual prices are stale by nature).
pub fn stale_price_alerts(
    positions: &[crate::domain::valuation::Position],
    instruments: &[crate::repo::instruments::InstrumentRow],
    today_wib: &str,
) -> Vec<Alert> {
    use std::collections::HashMap;
    let by_id: HashMap<i64, _> = instruments.iter().map(|i| (i.id, i)).collect();
    positions
        .iter()
        .filter(|p| p.price_stale && !p.quantity.is_zero())
        .filter_map(|p| by_id.get(&p.instrument_id))
        .filter(|i| i.price_source != "manual")
        .map(|i| Alert {
            dedup_key: format!("stale:{}:{today_wib}", i.symbol),
            message: format!(
                "⚠️ Harga {} tidak ter-update (sumber: {}) — connector mungkin bermasalah.",
                i.symbol, i.price_source
            ),
        })
        .collect()
}

/// Evaluate every trigger from stored data. Each section degrades
/// independently: one failing source must not silence the others.
pub async fn evaluate(
    db: &Db,
    mover_threshold_pct: f64,
    milestone_step_idr: i64,
    today_wib: &str,
) -> Vec<Alert> {
    let mut alerts = Vec::new();

    match crate::service::movers::daily_movers(db, 20).await {
        Ok(movers) => alerts.extend(mover_alerts(&movers, mover_threshold_pct, today_wib)),
        Err(e) => tracing::warn!("alerts: movers unavailable: {e:#}"),
    }

    match review_backlog_alert(db, today_wib).await {
        Ok(Some(alert)) => alerts.push(alert),
        Ok(None) => {}
        Err(e) => tracing::warn!("alerts: review backlog check failed: {e:#}"),
    }

    match crate::service::portfolio::build_summary(db).await {
        Ok(summary) => {
            match crate::repo::instruments::list(db).await {
                Ok(instruments) => {
                    alerts.extend(stale_price_alerts(&summary.positions, &instruments, today_wib))
                }
                Err(e) => tracing::warn!("alerts: instruments unavailable: {e:#}"),
            }
            // Milestones: yesterday's snapshot (NOT today's hourly-overwritten
            // one) vs current net worth.
            match crate::repo::snapshots::history(db).await {
                Ok(rows) => {
                    // ~24h baseline (see snapshot_before doc): yesterday WIB.
                    let yesterday = chrono::NaiveDate::parse_from_str(today_wib, "%Y-%m-%d")
                        .map(|d| (d - chrono::Duration::days(1)).format("%Y-%m-%d").to_string())
                        .unwrap_or_else(|_| today_wib.to_string());
                    if let Some(prev) = super::snapshot_before(&rows, &yesterday) {
                        let prev_idr = prev.trunc().to_string().parse::<i64>().unwrap_or(0);
                        let curr_idr = summary
                            .net_worth_idr
                            .trunc()
                            .to_string()
                            .parse::<i64>()
                            .unwrap_or(0);
                        for milestone in
                            milestones_crossed(prev_idr, curr_idr, milestone_step_idr)
                        {
                            alerts.push(milestone_alert(milestone));
                        }
                    }
                }
                Err(e) => tracing::warn!("alerts: snapshots unavailable: {e:#}"),
            }
        }
        Err(e) => tracing::warn!("alerts: portfolio summary unavailable: {e:#}"),
    }

    alerts
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn mover(symbol: &str, pct: f64, delta_idr: Decimal) -> Mover {
        Mover {
            instrument_id: 1,
            symbol: symbol.into(),
            name: symbol.into(),
            delta_idr,
            delta_pct: pct,
            value_idr: dec!(1000000),
        }
    }

    #[test]
    fn movers_below_threshold_are_silent() {
        let alerts = mover_alerts(&[mover("BBCA", 4.9, dec!(10000))], 5.0, "2026-06-12");
        assert!(alerts.is_empty());
    }

    #[test]
    fn movers_at_or_above_threshold_alert_in_both_directions() {
        let alerts = mover_alerts(
            &[mover("BBCA", 6.2, dec!(21000)), mover("BTC", -5.0, dec!(-540000))],
            5.0,
            "2026-06-12",
        );
        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0].dedup_key, "mover:BBCA:2026-06-12");
        assert!(alerts[0].message.contains("📈"), "{}", alerts[0].message);
        assert!(alerts[0].message.contains("BBCA"), "{}", alerts[0].message);
        assert!(alerts[0].message.contains("+6,2%"), "{}", alerts[0].message);
        assert!(alerts[0].message.contains("21.000"), "{}", alerts[0].message);
        assert!(alerts[1].message.contains("📉"), "{}", alerts[1].message);
        assert!(alerts[1].message.contains("-5,0%"), "{}", alerts[1].message);
    }

    #[test]
    fn milestones_crossed_finds_every_step_in_between() {
        // 1.49M -> 1.56M with 50jt steps crosses 1.50 and 1.55 (in millions of thousands).
        assert_eq!(
            milestones_crossed(1_490_000_000, 1_560_000_000, 50_000_000),
            vec![1_500_000_000, 1_550_000_000]
        );
        // No crossing.
        assert!(milestones_crossed(1_510_000_000, 1_540_000_000, 50_000_000).is_empty());
        // Downward movement never alerts.
        assert!(milestones_crossed(1_560_000_000, 1_490_000_000, 50_000_000).is_empty());
        // Exact landing on a milestone counts.
        assert_eq!(
            milestones_crossed(1_499_999_999, 1_500_000_000, 50_000_000),
            vec![1_500_000_000]
        );
        // Degenerate step.
        assert!(milestones_crossed(1, 2, 0).is_empty());
    }

    #[test]
    fn milestone_alert_message_formats_idr() {
        let alert = milestone_alert(1_550_000_000);
        assert_eq!(alert.dedup_key, "milestone:1550000000");
        assert!(alert.message.contains("1.550.000.000"), "{}", alert.message);
        assert!(alert.message.contains("🎉"), "{}", alert.message);
    }

    #[tokio::test]
    async fn review_backlog_alerts_once_per_day_only_when_old_items_exist() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        // Fresh pending item (now): no alert.
        seed_review_item(&db, &chrono::Utc::now().to_rfc3339()).await;
        assert!(review_backlog_alert(&db, "2026-06-12").await.unwrap().is_none());
        // Old pending item (>24h): alert with count.
        let old = (chrono::Utc::now() - chrono::Duration::hours(25)).to_rfc3339();
        seed_review_item(&db, &old).await;
        let alert = review_backlog_alert(&db, "2026-06-12").await.unwrap().expect("alert");
        assert_eq!(alert.dedup_key, "review-backlog:2026-06-12");
        assert!(alert.message.contains('1'), "{}", alert.message);
    }

    async fn seed_review_item(db: &Db, created_at: &str) {
        sqlx::query(
            "INSERT INTO review_item (batch_id, source_kind, source_filename, source_path,
             doc_type, status, needs_attention, payload_json, raw_llm_json, created_at)
             VALUES ('b', 'image', 'f.jpg', '', 'txn_history', 'pending', 0, '{}', '{}', ?)",
        )
        .bind(created_at)
        .execute(db)
        .await
        .unwrap();
    }
}
