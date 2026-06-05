use crate::repo::instruments::InstrumentRow;
use crate::service::portfolio::PortfolioSummary;
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Format a Decimal in Indonesian style: dots for thousands, comma for the
/// fraction, trailing zeros stripped (91960083 -> "91.960.083", 0.00052 ->
/// "0,00052"). Matches the web UI's id-ID Intl formatting so the model sees
/// and echoes numbers the way the rest of the app shows them.
pub(crate) fn group_id(d: &Decimal) -> String {
    let normalized = d.normalize();
    let digits = normalized.abs().to_string();
    let (int_part, frac_part) = match digits.split_once('.') {
        Some((int_digits, frac_digits)) => (int_digits, Some(frac_digits)),
        None => (digits.as_str(), None),
    };
    let mut out = String::new();
    if normalized.is_sign_negative() {
        out.push('-');
    }
    for (idx, ch) in int_part.chars().enumerate() {
        if idx > 0 && (int_part.len() - idx) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    if let Some(frac_digits) = frac_part {
        out.push(',');
        out.push_str(frac_digits);
    }
    out
}

/// IDR amounts: whole rupiah, grouped.
fn fmt_idr(d: &Decimal) -> String {
    group_id(&d.round_dp(0))
}

/// USD amounts: cents precision, grouped.
fn fmt_usd(d: &Decimal) -> String {
    group_id(&d.round_dp(2))
}

/// Render a compact text snapshot of the portfolio for the LLM context.
///
/// `instruments` supplies human-readable labels — holdings are listed as
/// "SYMBOL (Name)" so the model never has to answer with raw instrument ids.
pub fn build_context(s: &PortfolioSummary, instruments: &[InstrumentRow]) -> String {
    let labels: HashMap<i64, String> = instruments
        .iter()
        .map(|i| (i.id, format!("{} ({})", i.symbol, i.name)))
        .collect();
    let mut out = String::new();
    out.push_str(&format!("Net worth: Rp {} (USD {}).\n", fmt_idr(&s.net_worth_idr), fmt_usd(&s.net_worth_usd)));
    out.push_str(&format!("Unrealized P&L (IDR): {}. Realized P&L (IDR): {}.\n", fmt_idr(&s.total_unrealized_pnl_idr), fmt_idr(&s.total_realized_pnl_idr)));
    match s.xirr { Some(x) => out.push_str(&format!("XIRR: {:.1}%\n", x * 100.0)), None => out.push_str("XIRR: n/a\n") }
    out.push_str("Allocation (actual% / target%):\n");
    for c in &s.allocation {
        out.push_str(&format!("- {}: {}% / {}%{}\n", c.name, c.actual_pct, c.target_pct, if c.out_of_band { " (OUT OF BAND)" } else { "" }));
    }
    out.push_str("Holdings:\n");
    for p in &s.positions {
        let label = labels
            .get(&p.instrument_id)
            .cloned()
            .unwrap_or_else(|| format!("instrument#{}", p.instrument_id));
        out.push_str(&format!("- {}: qty {} value Rp {}\n", label, group_id(&p.quantity), fmt_idr(&p.market_value_idr)));
    }
    out
}

use crate::db::Db;
use crate::llm::claude::ClaudeClient;
use crate::repo::chat;

const SYSTEM: &str = "You are a concise personal investment assistant. Answer the user's question using ONLY the portfolio snapshot provided. Amounts are in IDR unless noted. If the snapshot lacks the info, say so briefly. Keep answers short. Format every number with Indonesian separators: dots for thousands, comma for decimals (e.g. Rp 91.960.083). Use the recent conversation to resolve follow-up questions. If pending review items are listed you may discuss them, but you cannot edit or confirm them — direct the user to the buttons in chat or the web UI Data page.";

/// How many prior messages of the channel's conversation the model sees.
const HISTORY_LIMIT: i64 = 12;

/// Render pending review items for the LLM context so the assistant knows
/// about staged-but-unconfirmed transactions (it cannot act on them).
fn render_pending_reviews(items: &[crate::repo::review_items::ReviewItemRow]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\nPending review items (user confirms via chat buttons or web UI Data page; you CANNOT edit or confirm them):\n",
    );
    for item in items {
        let entry: Option<crate::ingestion::extract::ExtractedEntry> =
            serde_json::from_str(&item.payload_json).ok();
        match entry {
            Some(e) => out.push_str(&format!(
                "- #{} {} {} qty {} price {}\n",
                item.id,
                e.entry_type,
                e.symbol.as_deref().unwrap_or("?"),
                e.quantity.as_deref().unwrap_or("?"),
                e.price_native.as_deref().unwrap_or("?"),
            )),
            None => out.push_str(&format!("- #{}\n", item.id)),
        }
    }
    out
}

/// Telegram/WhatsApp render replies as raw text, so Markdown tables and
/// **bold** markers show up literally — instruct plain text there. The in-app
/// chat renders Markdown (see frontend MarkdownMessage) and keeps the base prompt.
const PLAIN_TEXT_NOTE: &str = " You are replying inside a plain-text messenger: do NOT use any Markdown (no tables, no headers, no **bold**, no horizontal rules). Write short lines; for lists use simple dashes or emoji.";

/// System prompt for a channel: messengers get plain-text formatting rules.
fn system_prompt(channel: &str) -> String {
    match channel {
        "inapp" => SYSTEM.to_string(),
        _ => format!("{SYSTEM}{PLAIN_TEXT_NOTE}"),
    }
}

/// Build context (snapshot + pending reviews), replay the channel's recent
/// conversation, ask Claude, then store BOTH messages only on success
/// (avoids orphaned user msgs).
pub async fn answer(db: &Db, client: &ClaudeClient, channel: &str, user_msg: &str) -> anyhow::Result<String> {
    let summary = crate::service::portfolio::build_summary(db).await?;
    let instruments = crate::repo::instruments::list(db).await?;
    let mut context = build_context(&summary, &instruments);
    let pending = crate::repo::review_items::list_by_status(db, "pending")
        .await
        .unwrap_or_default();
    context.push_str(&render_pending_reviews(&pending));

    let system = format!("{}\n\nPortfolio snapshot:\n{context}", system_prompt(channel));
    let history = chat::recent_by_channel(db, channel, HISTORY_LIMIT)
        .await
        .unwrap_or_default();
    let mut turns: Vec<(String, String)> =
        history.into_iter().map(|m| (m.role, m.content)).collect();
    turns.push(("user".to_string(), user_msg.to_string()));

    let reply = client
        .complete_chat(&system, &turns)
        .await
        .map_err(|e| anyhow::anyhow!("llm error: {e}"))?;
    chat::add(db, "user", user_msg, channel).await?;
    chat::add(db, "assistant", &reply, channel).await?;
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn summary() -> PortfolioSummary {
        PortfolioSummary {
            net_worth_idr: dec!(4875000), net_worth_usd: dec!(300),
            total_unrealized_pnl_idr: dec!(100), total_realized_pnl_idr: dec!(0),
            xirr: Some(0.168), positions: vec![], allocation: vec![],
        }
    }

    fn position(instrument_id: i64) -> crate::domain::valuation::Position {
        crate::domain::valuation::Position {
            instrument_id,
            quantity: dec!(100), avg_cost: dec!(9000), cost_basis_total: dec!(900000),
            latest_price: dec!(10000), price_stale: false,
            market_value_native: dec!(1000000), market_value_idr: dec!(1000000),
            market_value_usd: dec!(62), unrealized_pnl: dec!(100000),
            realized_pnl: dec!(0), income: dec!(0),
        }
    }

    fn instrument_row(id: i64, symbol: &str, name: &str) -> crate::repo::instruments::InstrumentRow {
        crate::repo::instruments::InstrumentRow {
            id, symbol: symbol.into(), name: name.into(),
            instrument_type: "stock".into(), native_currency: "IDR".into(),
            category_id: None, price_source: "manual".into(), decimals: 0, note: None,
        }
    }

    #[test]
    fn grouping_formats_indonesian_thousands() {
        assert_eq!(group_id(&dec!(91960083)), "91.960.083");
        assert_eq!(group_id(&dec!(-13964613)), "-13.964.613");
        assert_eq!(group_id(&dec!(0.00052)), "0,00052");
        assert_eq!(group_id(&dec!(1234.50)), "1.234,5");
        assert_eq!(group_id(&dec!(100)), "100");
    }

    #[test]
    fn context_includes_net_worth_and_xirr() {
        let ctx = build_context(&summary(), &[]);
        assert!(ctx.contains("Net worth: Rp 4.875.000"), "{ctx}");
        assert!(ctx.contains("XIRR: 16.8%"));
    }

    #[test]
    fn context_handles_null_xirr() {
        let mut s = summary(); s.xirr = None;
        assert!(build_context(&s, &[]).contains("XIRR: n/a"));
    }

    #[test]
    fn context_labels_holdings_with_symbol_and_name() {
        let mut s = summary();
        s.positions = vec![position(3)];
        let ctx = build_context(&s, &[instrument_row(3, "BBCA", "Bank Central Asia")]);
        assert!(ctx.contains("- BBCA (Bank Central Asia): qty 100 value Rp 1.000.000"), "{ctx}");
        assert!(!ctx.contains("instrument#3"), "{ctx}");
    }

    #[test]
    fn context_falls_back_to_id_for_unknown_instruments() {
        let mut s = summary();
        s.positions = vec![position(7)];
        let ctx = build_context(&s, &[]);
        assert!(ctx.contains("- instrument#7: qty 100 value Rp 1.000.000"), "{ctx}");
    }

    fn pending_item(payload_json: &str) -> crate::repo::review_items::ReviewItemRow {
        crate::repo::review_items::ReviewItemRow {
            id: 75,
            batch_id: "tg-1".into(),
            source_kind: "image".into(),
            source_filename: "telegram-photo.jpg".into(),
            source_path: "".into(),
            doc_type: "holdings_snapshot".into(),
            status: "pending".into(),
            needs_attention: 0,
            payload_json: payload_json.into(),
            raw_llm_json: "{}".into(),
            suggested_instrument_id: None,
            suggested_account_id: None,
            created_txn_id: None,
            created_at: "2026-06-05T00:00:00Z".into(),
            confirmed_at: None,
        }
    }

    #[test]
    fn pending_reviews_render_with_their_details() {
        let item = pending_item(
            r#"{ "entry_type": "opening_balance", "symbol": "USDT", "quantity": "5661.940057" }"#,
        );
        let rendered = render_pending_reviews(&[item]);
        assert!(rendered.contains("#75"), "{rendered}");
        assert!(rendered.contains("opening_balance USDT"), "{rendered}");
        assert!(rendered.contains("5661.940057"), "{rendered}");
        assert!(rendered.to_lowercase().contains("cannot edit"), "{rendered}");
    }

    #[test]
    fn no_pending_reviews_renders_nothing() {
        assert_eq!(render_pending_reviews(&[]), "");
    }

    #[test]
    fn messenger_channels_get_plain_text_instructions() {
        for channel in ["telegram", "whatsapp"] {
            let prompt = system_prompt(channel);
            assert!(prompt.contains("plain-text"), "{channel} prompt must forbid Markdown");
            assert!(prompt.starts_with(SYSTEM), "{channel} prompt must keep the base instructions");
        }
    }

    #[test]
    fn inapp_channel_keeps_the_markdown_capable_prompt() {
        assert_eq!(system_prompt("inapp"), SYSTEM);
    }

    #[tokio::test]
    #[ignore]
    async fn live_answer_smoke() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let client = match crate::llm::claude::ClaudeClient::from_env() { Ok(c) => c, Err(_) => return };
        let reply = answer(&db, &client, "inapp", "What is my net worth?").await.unwrap();
        assert!(!reply.is_empty());
        assert_eq!(crate::repo::chat::history(&db, 10).await.unwrap().len(), 2);
    }
}
