//! Weekly recap: deterministic gathering, then compose-and-send.

use crate::db::Db;
use crate::service::chat::group_id;
use chrono::Datelike;
use rust_decimal::Decimal;

pub struct RecapData {
    pub week_label: String,
    pub todos_completed: usize,
    pub todos_created: i64,
    pub reminders_sent: i64,
    pub net_worth_idr: Decimal,
    pub week_delta_idr: Option<Decimal>,
    pub spending_idr: Decimal,
    pub spending_skipped_non_idr: usize,
    pub movers: Vec<crate::service::movers::Mover>,
    pub todos_next_week: Vec<crate::repo::todos::TodoRow>,
    pub reminders_next_week: Vec<crate::repo::reminders::ReminderRow>,
    pub events_next_week: Vec<crate::repo::events::EventRow>,
    /// Hyperliquid equity snapshot vs. one week ago. `None` when the instrument
    /// is not configured or no price data is available.
    pub hyperliquid: Option<crate::service::hyperliquid::HlEquitySummary>,
}

/// Sum IDR outflows whose occurred_on is at/after `since_date` (YYYY-MM-DD).
/// Returns (total, count of skipped non-IDR outflows in the window).
pub fn weekly_spending_idr(
    rows: &[crate::repo::cashflow::CashflowRow],
    since_date: &str,
) -> (Decimal, usize) {
    use std::str::FromStr;
    let mut total = Decimal::ZERO;
    let mut skipped = 0usize;
    for row in rows {
        if row.direction != "out" || row.occurred_on.as_str() < since_date {
            continue;
        }
        if row.currency != "IDR" {
            skipped += 1;
            continue;
        }
        total += Decimal::from_str(&row.amount).unwrap_or_default();
    }
    (total, skipped)
}

/// Gather the week's numbers. Finance sources degrade independently.
pub async fn gather(db: &Db) -> anyhow::Result<RecapData> {
    let now = chrono::Utc::now();
    let now_wib = now.with_timezone(&crate::assistant::time::wib());
    let week = now_wib.iso_week();
    // Two formats on purpose: each column is compared in the format its writer uses.
    let week_ago_rfc3339 = (now - chrono::Duration::days(7)).to_rfc3339();
    let week_ago_z = crate::assistant::time::to_db_utc(now - chrono::Duration::days(7));
    let week_ago_date = (now_wib - chrono::Duration::days(7)).format("%Y-%m-%d").to_string();
    let next_week_end = crate::assistant::time::to_db_utc(now + chrono::Duration::days(7));
    let now_z = crate::assistant::time::to_db_utc(now);

    let todos_completed = crate::repo::todos::completed_since(db, &week_ago_rfc3339).await?.len();
    let todos_created = crate::repo::todos::created_count_since(db, &week_ago_rfc3339).await?;
    let reminders_sent = crate::repo::reminders::sent_count_since(db, &week_ago_z).await?;

    let (net_worth_idr, week_delta_idr) = match crate::service::portfolio::build_summary(db).await {
        Ok(summary) => {
            let delta = match crate::repo::snapshots::history(db).await {
                Ok(rows) => {
                    use std::str::FromStr;
                    rows.iter()
                        .rev()
                        .find(|r| r.as_of.as_str() <= week_ago_date.as_str())
                        .or(rows.first())
                        .and_then(|r| Decimal::from_str(&r.total_idr).ok())
                        .map(|start| summary.net_worth_idr - start)
                }
                Err(_) => None,
            };
            (summary.net_worth_idr, delta)
        }
        Err(e) => {
            tracing::warn!("recap: portfolio summary unavailable: {e:#}");
            (Decimal::ZERO, None)
        }
    };

    let (spending_idr, spending_skipped_non_idr) = match crate::repo::cashflow::list_all(db).await {
        Ok(rows) => weekly_spending_idr(&rows, &week_ago_date),
        Err(e) => {
            tracing::warn!("recap: cashflow unavailable: {e:#}");
            (Decimal::ZERO, 0)
        }
    };

    let movers = crate::service::movers::daily_movers(db, 3).await.unwrap_or_else(|e| {
        tracing::warn!("recap: movers unavailable: {e:#}");
        Vec::new()
    });

    let todos_next_week = crate::repo::todos::list_open(db)
        .await?
        .into_iter()
        .filter(|t| {
            t.due_at
                .as_deref()
                .map(|d| d >= now_z.as_str() && d <= next_week_end.as_str())
                .unwrap_or(false)
        })
        .collect();
    let reminders_next_week = crate::repo::reminders::list_pending(db)
        .await?
        .into_iter()
        .filter(|r| r.remind_at.as_str() >= now_z.as_str() && r.remind_at.as_str() <= next_week_end.as_str())
        .collect();
    let events_next_week =
        crate::repo::events::list_between(db, &now_z, &next_week_end).await?;

    let hyperliquid = crate::service::hyperliquid::equity_and_change(db, &week_ago_date)
        .await
        .unwrap_or(None);

    Ok(RecapData {
        week_label: format!("{}-W{:02}", week.year(), week.week()),
        todos_completed,
        todos_created,
        reminders_sent,
        net_worth_idr,
        week_delta_idr,
        spending_idr,
        spending_skipped_non_idr,
        movers,
        todos_next_week,
        reminders_next_week,
        events_next_week,
        hyperliquid,
    })
}

/// Deterministic data block: LLM input and fallback body.
pub fn render_data_block(d: &RecapData) -> String {
    let mut out = format!("Rekap minggu {}\n", d.week_label);
    out.push_str(&format!(
        "Todo selesai: {} (dibuat: {})\n",
        d.todos_completed, d.todos_created
    ));
    out.push_str(&format!("Reminder terkirim: {}\n", d.reminders_sent));
    out.push_str(&format!(
        "Net worth: Rp {}\n",
        group_id(&d.net_worth_idr.round_dp(0))
    ));
    if let Some(delta) = &d.week_delta_idr {
        let sign = if delta.is_sign_negative() { "-" } else { "+" };
        out.push_str(&format!(
            "Perubahan seminggu: {sign}Rp {}\n",
            group_id(&delta.abs().round_dp(0))
        ));
    }
    out.push_str(&format!(
        "Pengeluaran minggu ini: Rp {}\n",
        group_id(&d.spending_idr.round_dp(0))
    ));
    if d.spending_skipped_non_idr > 0 {
        out.push_str(&format!(
            "(catatan: {} transaksi non-IDR tidak ikut dijumlahkan)\n",
            d.spending_skipped_non_idr
        ));
    }
    if !d.movers.is_empty() {
        out.push_str("Movers terakhir:\n");
        for m in &d.movers {
            let sign = if m.delta_idr.is_sign_negative() { "-" } else { "+" };
            out.push_str(&format!(
                "- {} {}: {sign}Rp {}\n",
                m.symbol,
                format!("{:+.1}%", m.delta_pct).replace('.', ","),
                group_id(&m.delta_idr.abs().round_dp(0)),
            ));
        }
    }
    if let Some(hl) = &d.hyperliquid {
        out.push_str(&crate::service::hyperliquid::format_hyperliquid_line(hl));
        out.push('\n');
    }
    out.push_str("Minggu depan:\n");
    if d.todos_next_week.is_empty()
        && d.reminders_next_week.is_empty()
        && d.events_next_week.is_empty()
    {
        out.push_str("(tidak ada jadwal tercatat)\n");
    } else {
        for e in &d.events_next_week {
            out.push_str(&format!(
                "- event: {} ({})\n",
                e.title,
                crate::assistant::time::to_wib_display(&e.start_at)
            ));
        }
        for t in &d.todos_next_week {
            out.push_str(&format!("- todo #{} {}", t.id, t.title));
            if let Some(due) = &t.due_at {
                out.push_str(&format!(
                    " (due {})",
                    crate::assistant::time::to_wib_display(due)
                ));
            }
            out.push('\n');
        }
        for r in &d.reminders_next_week {
            out.push_str(&format!(
                "- reminder: {} ({})\n",
                r.message,
                crate::assistant::time::to_wib_display(&r.remind_at)
            ));
        }
    }
    out
}

/// Gather → compose → send. The caller has already claimed the dedup key.
pub async fn run(
    db: &Db,
    client: &crate::telegram::client::TelegramClient,
    chat_id: i64,
) -> anyhow::Result<()> {
    let data = gather(db).await?;
    let block = render_data_block(&data);
    let text = super::compose::compose(
        super::compose::RECAP_SYSTEM,
        &block,
        "📊 Rekap mingguan (mode ringkas)",
    )
    .await;
    client
        .send_message(chat_id, &text)
        .await
        .map_err(|e| anyhow::anyhow!("recap send failed: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn weekly_spending_sums_idr_outflows_in_window() {
        let rows = vec![
            cf("2026-06-08", "out", "150000", "IDR"),
            cf("2026-06-10", "out", "50000", "IDR"),
            cf("2026-06-10", "in", "9000000", "IDR"),  // income ignored
            cf("2026-06-01", "out", "999999", "IDR"),  // before window
            cf("2026-06-10", "out", "20", "USD"),      // non-IDR skipped
        ];
        let (total, skipped) = weekly_spending_idr(&rows, "2026-06-07");
        assert_eq!(total, dec!(200000));
        assert_eq!(skipped, 1);
    }

    fn cf(on: &str, dir: &str, amount: &str, currency: &str) -> crate::repo::cashflow::CashflowRow {
        crate::repo::cashflow::CashflowRow {
            id: 0,
            account_id: None,
            occurred_on: on.into(),
            direction: dir.into(),
            amount: amount.into(),
            currency: currency.into(),
            category_id: None,
            note: None,
            source: None,
            external_ref: None,
            created_at: String::new(),
        }
    }

    #[test]
    fn block_renders_productivity_finance_and_next_week() {
        let d = RecapData {
            week_label: "2026-W24".into(),
            todos_completed: 4,
            todos_created: 6,
            reminders_sent: 3,
            net_worth_idr: dec!(92000000),
            week_delta_idr: Some(dec!(-500000)),
            spending_idr: dec!(200000),
            spending_skipped_non_idr: 1,
            movers: vec![],
            todos_next_week: vec![],
            reminders_next_week: vec![],
            events_next_week: vec![],
            hyperliquid: None,
        };
        let block = render_data_block(&d);
        assert!(block.contains("Todo selesai: 4 (dibuat: 6)"), "{block}");
        assert!(block.contains("Reminder terkirim: 3"), "{block}");
        assert!(block.contains("-Rp 500.000"), "{block}");
        assert!(block.contains("Rp 200.000"), "{block}");
        assert!(block.contains("1 transaksi non-IDR"), "{block}");
        assert!(block.contains("Minggu depan"), "{block}");
    }

    #[tokio::test]
    async fn gather_works_on_an_empty_db() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let d = gather(&db).await.unwrap();
        assert_eq!(d.todos_completed, 0);
        assert_eq!(d.reminders_sent, 0);
        assert_eq!(d.spending_idr, Decimal::ZERO);
    }

    #[test]
    fn next_week_section_includes_events() {
        let mut d = RecapData {
            week_label: "2026-W24".into(),
            todos_completed: 0,
            todos_created: 0,
            reminders_sent: 0,
            net_worth_idr: dec!(0),
            week_delta_idr: None,
            spending_idr: dec!(0),
            spending_skipped_non_idr: 0,
            movers: vec![],
            todos_next_week: vec![],
            reminders_next_week: vec![],
            events_next_week: vec![],
            hyperliquid: None,
        };
        d.events_next_week = vec![crate::repo::events::EventRow {
            id: 1,
            title: "kontrol gigi".into(),
            location: None,
            notes: None,
            start_at: "2026-06-17T02:00:00Z".into(),
            status: "scheduled".into(),
            created_at: String::new(),
            source: "local".into(),
            google_event_id: None,
            google_etag: None,
            synced_at: None,
            updated_at: None,
        }];
        let block = render_data_block(&d);
        assert!(block.contains("- event: kontrol gigi"), "{block}");
        assert!(!block.contains("(tidak ada jadwal tercatat)"), "{block}");
    }
}
