use crate::repo::instruments::InstrumentRow;
use crate::service::portfolio::PortfolioSummary;
use std::collections::HashMap;

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
    out.push_str(&format!("Net worth: Rp {} (USD {}).\n", s.net_worth_idr, s.net_worth_usd));
    out.push_str(&format!("Unrealized P&L (IDR): {}. Realized P&L (IDR): {}.\n", s.total_unrealized_pnl_idr, s.total_realized_pnl_idr));
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
        out.push_str(&format!("- {}: qty {} value Rp {}\n", label, p.quantity, p.market_value_idr));
    }
    out
}

use crate::db::Db;
use crate::llm::claude::{ClaudeClient, Part};
use crate::repo::chat;

const SYSTEM: &str = "You are a concise personal investment assistant. Answer the user's question using ONLY the portfolio snapshot provided. Amounts are in IDR unless noted. If the snapshot lacks the info, say so briefly. Keep answers short.";

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

/// Build context, ask Claude, then store BOTH messages only on success (avoids orphaned user msgs).
pub async fn answer(db: &Db, client: &ClaudeClient, channel: &str, user_msg: &str) -> anyhow::Result<String> {
    let summary = crate::service::portfolio::build_summary(db).await?;
    let instruments = crate::repo::instruments::list(db).await?;
    let context = build_context(&summary, &instruments);
    let prompt = format!("Portfolio snapshot:\n{context}\n\nUser question: {user_msg}");
    let reply = client.complete(&system_prompt(channel), &[Part::Text(prompt)]).await
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
    fn context_includes_net_worth_and_xirr() {
        let ctx = build_context(&summary(), &[]);
        assert!(ctx.contains("Net worth: Rp 4875000"));
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
        assert!(ctx.contains("- BBCA (Bank Central Asia): qty 100 value Rp 1000000"), "{ctx}");
        assert!(!ctx.contains("instrument#3"), "{ctx}");
    }

    #[test]
    fn context_falls_back_to_id_for_unknown_instruments() {
        let mut s = summary();
        s.positions = vec![position(7)];
        let ctx = build_context(&s, &[]);
        assert!(ctx.contains("- instrument#7: qty 100 value Rp 1000000"), "{ctx}");
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
