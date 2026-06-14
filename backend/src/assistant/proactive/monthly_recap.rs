//! Monthly recap: deterministic gathering for the prior month, then compose-and-send.
//! Fires on the 1st of each month (recapping the month that just ended).

use crate::db::Db;
use crate::service::chat::group_id;
use chrono::{DateTime, Datelike, Utc};
use rust_decimal::Decimal;

pub struct MonthlyRecapData {
    /// Label for the month being recapped, e.g. "2026-06".
    pub month_label: String,
    /// Number of todos completed during the prior month.
    pub todos_done: usize,
    /// Total IDR money received during the prior month (cashflow "in", IDR only).
    pub money_in_idr: Decimal,
    /// Total IDR money spent during the prior month (cashflow "out", IDR only).
    pub money_out_idr: Decimal,
    /// Net cashflow for the prior month (money_in - money_out), IDR only.
    pub net_idr: Decimal,
    /// Total freelance invoiced during the prior month (sum of invoice totals by issue_date).
    pub freelance_invoiced_idr: Decimal,
    /// Net-worth change over the prior month (end-of-month minus start-of-month snapshot).
    pub net_worth_change_idr: Option<Decimal>,
}

/// Compute the prior-month label (YYYY-MM) given a UTC instant.
/// "Prior month" is always relative to the WIB calendar month containing `now_utc`.
pub fn prior_month_label(now_utc: DateTime<Utc>) -> String {
    let now_wib = now_utc.with_timezone(&crate::assistant::time::wib());
    let (year, month) = if now_wib.month() == 1 {
        (now_wib.year() - 1, 12u32)
    } else {
        (now_wib.year(), now_wib.month() - 1)
    };
    format!("{year}-{month:02}")
}

/// Gather the prior month's numbers. Finance sources degrade independently.
pub async fn gather(db: &Db, now_utc: DateTime<Utc>) -> anyhow::Result<MonthlyRecapData> {
    let month_label = prior_month_label(now_utc);

    // --- Todos completed during the prior month ---
    // `completed_since` returns all todos completed at/after the given RFC3339 date.
    // We filter to those whose completed_at starts with the prior-month prefix.
    let month_prefix = month_label.clone();
    let month_start_rfc3339 = format!("{month_label}-01T00:00:00+00:00");
    let todos_done = crate::repo::todos::completed_since(db, &month_start_rfc3339)
        .await?
        .into_iter()
        .filter(|t| {
            t.completed_at
                .as_deref()
                .map(|at| at.starts_with(&month_prefix))
                .unwrap_or(false)
        })
        .count();

    // --- Cashflow for the prior month (IDR only) ---
    let (money_in_idr, money_out_idr, net_idr) =
        match crate::repo::cashflow::list_for_month(db, &month_label).await {
            Ok(rows) => {
                let mut total_in = Decimal::ZERO;
                let mut total_out = Decimal::ZERO;
                for row in &rows {
                    if row.currency != "IDR" {
                        continue;
                    }
                    use std::str::FromStr;
                    let amount = Decimal::from_str(&row.amount).unwrap_or_default();
                    if row.direction == "in" {
                        total_in += amount;
                    } else {
                        total_out += amount;
                    }
                }
                (total_in, total_out, total_in - total_out)
            }
            Err(e) => {
                tracing::warn!("monthly_recap: cashflow unavailable: {e:#}");
                (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO)
            }
        };

    // --- Freelance invoiced: sum invoice totals whose issue_date is in the prior month ---
    let freelance_invoiced_idr =
        match crate::repo::invoices::list_all(db).await {
            Ok(rows) => {
                use std::str::FromStr;
                rows.iter()
                    .filter(|inv| inv.issue_date.starts_with(&month_label))
                    .filter_map(|inv| Decimal::from_str(&inv.total).ok())
                    .fold(Decimal::ZERO, |acc, v| acc + v)
            }
            Err(e) => {
                tracing::warn!("monthly_recap: invoices unavailable: {e:#}");
                Decimal::ZERO
            }
        };

    // --- Net-worth change: snapshot at end of prior month minus snapshot at start ---
    let net_worth_change_idr = match crate::repo::snapshots::history(db).await {
        Ok(rows) => {
            use std::str::FromStr;
            // End-of-month snapshot: latest row whose as_of starts with the prior month.
            let end_snap = rows
                .iter()
                .filter(|r| r.as_of.starts_with(&month_label))
                .last()
                .and_then(|r| Decimal::from_str(&r.total_idr).ok());
            // Start-of-month baseline: latest row strictly before the prior month.
            let start_snap = rows
                .iter()
                .rev()
                .find(|r| r.as_of.as_str() < format!("{month_label}-01").as_str())
                .and_then(|r| Decimal::from_str(&r.total_idr).ok());
            match (end_snap, start_snap) {
                (Some(end), Some(start)) => Some(end - start),
                _ => None,
            }
        }
        Err(e) => {
            tracing::warn!("monthly_recap: snapshots unavailable: {e:#}");
            None
        }
    };

    Ok(MonthlyRecapData {
        month_label,
        todos_done,
        money_in_idr,
        money_out_idr,
        net_idr,
        freelance_invoiced_idr,
        net_worth_change_idr,
    })
}

/// Deterministic data block: LLM input and fallback body.
pub fn render_data_block(d: &MonthlyRecapData) -> String {
    let mut out = format!("Rekap bulanan {}\n", d.month_label);

    // Productivity section.
    if d.todos_done > 0 {
        out.push_str(&format!("Todo selesai: {}\n", d.todos_done));
    } else {
        out.push_str("Todo selesai: (tidak ada)\n");
    }

    // Finances: money in/out/net.
    out.push_str(&format!(
        "Pemasukan: Rp {}\n",
        group_id(&d.money_in_idr.round_dp(0))
    ));
    out.push_str(&format!(
        "Pengeluaran: Rp {}\n",
        group_id(&d.money_out_idr.round_dp(0))
    ));
    let net_sign = if d.net_idr.is_sign_negative() { "-" } else { "+" };
    out.push_str(&format!(
        "Arus kas bersih: {net_sign}Rp {}\n",
        group_id(&d.net_idr.abs().round_dp(0))
    ));

    // Net-worth change (optional).
    if let Some(change) = &d.net_worth_change_idr {
        let sign = if change.is_sign_negative() { "-" } else { "+" };
        out.push_str(&format!(
            "Perubahan net worth: {sign}Rp {}\n",
            group_id(&change.abs().round_dp(0))
        ));
    }

    // Freelance invoiced (skip when zero).
    if d.freelance_invoiced_idr > Decimal::ZERO {
        out.push_str(&format!(
            "Freelance diinvoice: Rp {}\n",
            group_id(&d.freelance_invoiced_idr.round_dp(0))
        ));
    }

    out
}

/// Gather → compose → send. The caller has already claimed the dedup key.
pub async fn run(
    db: &Db,
    client: &crate::telegram::client::TelegramClient,
    chat_id: i64,
) -> anyhow::Result<()> {
    let data = gather(db, Utc::now()).await?;
    let block = render_data_block(&data);
    let text = super::compose::compose(
        super::compose::MONTHLY_RECAP_SYSTEM,
        &block,
        "📅 Recap bulanan (mode ringkas)",
    )
    .await;
    client
        .send_message(chat_id, &text)
        .await
        .map_err(|e| anyhow::anyhow!("monthly_recap send failed: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    // -------------------------------------------------------------------------
    // prior_month_label tests
    // -------------------------------------------------------------------------

    fn utc(y: i32, mo: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, 0, 0, 0).unwrap()
    }

    #[test]
    fn prior_month_mid_year() {
        // July 2026 → prior month is June 2026.
        assert_eq!(prior_month_label(utc(2026, 7, 1)), "2026-06");
        assert_eq!(prior_month_label(utc(2026, 7, 15)), "2026-06");
    }

    #[test]
    fn prior_month_january_wraps_to_december() {
        // January 2027 → prior month is December 2026.
        assert_eq!(prior_month_label(utc(2027, 1, 1)), "2026-12");
    }

    #[test]
    fn prior_month_december() {
        // December 2026 → prior month is November 2026.
        assert_eq!(prior_month_label(utc(2026, 12, 1)), "2026-11");
    }

    #[test]
    fn prior_month_wib_boundary() {
        // WIB is UTC+7; 2026-07-01T00:00:00Z is still 2026-07-01T07:00:00 WIB,
        // so prior month should still be 2026-06.
        assert_eq!(prior_month_label(utc(2026, 7, 1)), "2026-06");
    }

    // -------------------------------------------------------------------------
    // render_data_block tests
    // -------------------------------------------------------------------------

    #[test]
    fn render_block_contains_all_sections() {
        let d = MonthlyRecapData {
            month_label: "2026-06".into(),
            todos_done: 8,
            money_in_idr: dec!(5000000),
            money_out_idr: dec!(3000000),
            net_idr: dec!(2000000),
            freelance_invoiced_idr: dec!(12000000),
            net_worth_change_idr: Some(dec!(1500000)),
        };
        let block = render_data_block(&d);
        assert!(block.contains("Rekap bulanan 2026-06"), "{block}");
        assert!(block.contains("Todo selesai: 8"), "{block}");
        assert!(block.contains("Pemasukan: Rp 5.000.000"), "{block}");
        assert!(block.contains("Pengeluaran: Rp 3.000.000"), "{block}");
        assert!(block.contains("+Rp 2.000.000"), "{block}");
        assert!(block.contains("Perubahan net worth"), "{block}");
        assert!(block.contains("Freelance diinvoice: Rp 12.000.000"), "{block}");
    }

    #[test]
    fn render_block_skips_freelance_when_zero() {
        let d = MonthlyRecapData {
            month_label: "2026-06".into(),
            todos_done: 0,
            money_in_idr: dec!(0),
            money_out_idr: dec!(0),
            net_idr: dec!(0),
            freelance_invoiced_idr: dec!(0),
            net_worth_change_idr: None,
        };
        let block = render_data_block(&d);
        assert!(!block.contains("Freelance"), "{block}");
        assert!(!block.contains("Perubahan net worth"), "{block}");
    }

    #[test]
    fn render_block_shows_negative_net_with_minus_sign() {
        let d = MonthlyRecapData {
            month_label: "2026-05".into(),
            todos_done: 2,
            money_in_idr: dec!(1000000),
            money_out_idr: dec!(3000000),
            net_idr: dec!(-2000000),
            freelance_invoiced_idr: dec!(0),
            net_worth_change_idr: Some(dec!(-500000)),
        };
        let block = render_data_block(&d);
        assert!(block.contains("-Rp 2.000.000"), "{block}");
        assert!(block.contains("-Rp 500.000"), "{block}");
    }

    // -------------------------------------------------------------------------
    // gather integration test (empty DB)
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn gather_works_on_empty_db() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let now = utc(2026, 7, 1);
        let data = gather(&db, now).await.unwrap();
        assert_eq!(data.month_label, "2026-06");
        assert_eq!(data.todos_done, 0);
        assert_eq!(data.money_in_idr, Decimal::ZERO);
        assert_eq!(data.money_out_idr, Decimal::ZERO);
        assert_eq!(data.freelance_invoiced_idr, Decimal::ZERO);
        assert!(data.net_worth_change_idr.is_none());
    }
}
