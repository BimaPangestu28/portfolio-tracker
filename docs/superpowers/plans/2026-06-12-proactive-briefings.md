# Proactive Briefings, Alerts & Weekly Recap (Phase 4) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A 5-minute tick loop that sends a 07:00 WIB morning briefing, a Sunday 17:00 WIB weekly recap (both LLM-composed with a plain-text fallback), and event-driven financial alerts — all idempotent across restarts via a `proactive_log` dedup table.

**Architecture:** New `assistant/proactive/` module (tick / briefing / recap / alerts / compose, one responsibility each). Data gathering is deterministic and separate from LLM composition, which is separate from Telegram delivery. Insert-the-log-row-first gives at-most-once semantics (deliberate inverse of reminders). Reuses `service::movers`, `service::portfolio`, `repo::{todos,reminders,snapshots,cashflow,review_items}`, `assistant::memory`, `llm::claude`, `TelegramClient`.

**Tech Stack:** Rust (existing deps only — tokio/sqlx/chrono/reqwest/rust_decimal).

**Spec:** `docs/superpowers/specs/2026-06-12-proactive-briefings-design.md`

**Conventions:**
- Commands run from `backend/`. Commit after every task. Tests NEVER set env vars (no `BRIEFING_HOUR_WIB` etc. in tests — pure functions take parsed values instead).
- Timestamp formats: `todos.created_at`/`completed_at` are `to_rfc3339()` (+00:00); `reminders.sent_at` is `to_db_utc` (Z). When comparing in SQL, generate the `since` bound in the SAME format as the column (this matters — mixed formats break lexicographic order).
- Baseline before Task 1: `cargo test` = 314 passed. Verify, and trust your measured numbers over the expected counts below if they drift.

---

### Task 1: Migration + proactive_log repo

**Files:**
- Create: `backend/migrations/0012_proactive.sql`
- Create: `backend/src/repo/proactive_log.rs`
- Modify: `backend/src/repo/mod.rs`

- [ ] **Step 1: Write the migration**

`backend/migrations/0012_proactive.sql`:

```sql
-- Phase 4: dedup/idempotency log for proactive sends (briefing, recap, alerts).
-- sent_at is audit-only RFC3339 UTC.
CREATE TABLE proactive_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  kind TEXT NOT NULL,
  dedup_key TEXT NOT NULL UNIQUE,
  sent_at TEXT NOT NULL
);
```

(Migration number verified against current main: highest is 0011.)

- [ ] **Step 2: Write failing tests** — create `backend/src/repo/proactive_log.rs`:

```rust
//! Dedup log for proactive sends (see migration 0012). Claim-before-send:
//! a successful claim means "this dedup_key is now spoken for, forever".

use crate::db::Db;

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn first_claim_wins_second_loses() {
        let db = mem_db().await;
        assert!(try_claim(&db, "briefing", "briefing:2026-06-13").await.unwrap());
        assert!(!try_claim(&db, "briefing", "briefing:2026-06-13").await.unwrap());
    }

    #[tokio::test]
    async fn different_keys_claim_independently() {
        let db = mem_db().await;
        assert!(try_claim(&db, "alert", "mover:BBCA:2026-06-13").await.unwrap());
        assert!(try_claim(&db, "alert", "mover:BTC:2026-06-13").await.unwrap());
        assert!(try_claim(&db, "alert", "milestone:1550000000").await.unwrap());
        assert!(!try_claim(&db, "alert", "milestone:1550000000").await.unwrap());
    }
}
```

- [ ] **Step 3: Register and verify failure**

Add `pub mod proactive_log;` to `backend/src/repo/mod.rs`.
Run: `cd backend && cargo test repo::proactive_log`
Expected: COMPILE ERROR — `try_claim` not found.

- [ ] **Step 4: Implement** — insert between imports and tests:

```rust
/// Claim a dedup key. Returns true exactly once per key (INSERT OR IGNORE);
/// false means it was already claimed — by this run or any earlier one.
pub async fn try_claim(db: &Db, kind: &str, dedup_key: &str) -> anyhow::Result<bool> {
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "INSERT OR IGNORE INTO proactive_log (kind, dedup_key, sent_at) VALUES (?, ?, ?)",
    )
    .bind(kind)
    .bind(dedup_key)
    .bind(&now)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test repo::proactive_log` — expect 2 PASS. Then full `cargo test` — the new migration must not break anything (expect 316).

- [ ] **Step 6: Commit**

```bash
git add backend/migrations/0012_proactive.sql backend/src/repo/proactive_log.rs backend/src/repo/mod.rs
git commit -m "feat(proactive): add dedup log table and claim repo"
```

---

### Task 2: Module skeleton, config, due-window logic

**Files:**
- Create: `backend/src/assistant/proactive/mod.rs`
- Create: `backend/src/assistant/proactive/tick.rs`
- Modify: `backend/src/assistant/mod.rs` (add `pub mod proactive;`)

- [ ] **Step 1: Skeleton**

`backend/src/assistant/proactive/mod.rs`:

```rust
//! Proactive sends: morning briefing, weekly recap, financial alerts.
//! Deterministic gathering → LLM composition (with fallback) → Telegram.

pub mod tick;
```

Add `pub mod proactive;` to `backend/src/assistant/mod.rs`.

- [ ] **Step 2: Write failing tests** — `backend/src/assistant/proactive/tick.rs` starts with imports + tests:

```rust
//! 5-minute loop: claim due schedules, run jobs, evaluate alerts.

use crate::db::Db;
use crate::telegram::client::TelegramClient;
use chrono::{DateTime, Datelike, FixedOffset, Timelike};

#[cfg(test)]
mod tests {
    use super::*;

    fn wib(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<FixedOffset> {
        use chrono::TimeZone;
        crate::assistant::time::wib().with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    // 2026-06-12 is a Friday; 2026-06-14 is a Sunday; 2026-06-15 a Monday.

    #[test]
    fn briefing_due_inside_the_window_only() {
        assert_eq!(briefing_due(wib(2026, 6, 12, 6, 55), Some(7)), None);
        assert_eq!(
            briefing_due(wib(2026, 6, 12, 7, 0), Some(7)),
            Some("briefing:2026-06-12".to_string())
        );
        assert_eq!(
            briefing_due(wib(2026, 6, 12, 11, 59), Some(7)),
            Some("briefing:2026-06-12".to_string())
        );
        // Past the 5-hour grace window the day is forfeited.
        assert_eq!(briefing_due(wib(2026, 6, 12, 12, 0), Some(7)), None);
        // Disabled.
        assert_eq!(briefing_due(wib(2026, 6, 12, 8, 0), None), None);
    }

    #[test]
    fn recap_due_sunday_evening_with_monday_grace() {
        // Friday: never.
        assert_eq!(recap_due(wib(2026, 6, 12, 18, 0), Some(17)), None);
        // Sunday before the hour: not yet.
        assert_eq!(recap_due(wib(2026, 6, 14, 16, 59), Some(17)), None);
        // Sunday at/after the hour: due, keyed by the ISO week ending that Sunday.
        assert_eq!(
            recap_due(wib(2026, 6, 14, 17, 0), Some(17)),
            Some("recap:2026-W24".to_string())
        );
        // Monday before 09:00: grace, SAME key (the week that ended yesterday).
        assert_eq!(
            recap_due(wib(2026, 6, 15, 8, 30), Some(17)),
            Some("recap:2026-W24".to_string())
        );
        // Monday 09:00: forfeited.
        assert_eq!(recap_due(wib(2026, 6, 15, 9, 0), Some(17)), None);
        // Disabled.
        assert_eq!(recap_due(wib(2026, 6, 14, 18, 0), None), None);
    }

    #[test]
    fn config_defaults_are_sane() {
        // Tests never set the env vars, so this exercises the default path.
        let config = ProactiveConfig::from_env();
        assert_eq!(config.briefing_hour, Some(7));
        assert_eq!(config.recap_hour, Some(17));
        assert!((config.mover_alert_pct - 5.0).abs() < f64::EPSILON);
        assert_eq!(config.milestone_step_idr, 50_000_000);
    }

    #[test]
    fn hour_parsing_handles_off_and_garbage() {
        assert_eq!(parse_hour(None, 7), Some(7));
        assert_eq!(parse_hour(Some("off".into()), 7), None);
        assert_eq!(parse_hour(Some("OFF".into()), 7), None);
        assert_eq!(parse_hour(Some("9".into()), 7), Some(9));
        // Garbage and out-of-range fall back to the default.
        assert_eq!(parse_hour(Some("banana".into()), 7), Some(7));
        assert_eq!(parse_hour(Some("25".into()), 7), Some(7));
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cd backend && cargo test assistant::proactive` — expect COMPILE ERROR.

- [ ] **Step 4: Implement** — insert between imports and tests:

```rust
/// How long after the scheduled hour a send is still useful.
const GRACE_HOURS: u32 = 5;
/// Monday-morning cutoff for the weekly recap grace window.
const RECAP_MONDAY_GRACE_END_HOUR: u32 = 9;

#[derive(Debug, Clone)]
pub struct ProactiveConfig {
    pub briefing_hour: Option<u32>,
    pub recap_hour: Option<u32>,
    pub mover_alert_pct: f64,
    pub milestone_step_idr: i64,
}

/// "off" disables; unparseable or out-of-range values fall back to default.
fn parse_hour(raw: Option<String>, default: u32) -> Option<u32> {
    match raw {
        None => Some(default),
        Some(v) if v.eq_ignore_ascii_case("off") => None,
        Some(v) => v.parse().ok().filter(|h| *h < 24).or(Some(default)),
    }
}

impl ProactiveConfig {
    pub fn from_env() -> Self {
        Self {
            briefing_hour: parse_hour(std::env::var("BRIEFING_HOUR_WIB").ok(), 7),
            recap_hour: parse_hour(std::env::var("RECAP_HOUR_WIB").ok(), 17),
            mover_alert_pct: std::env::var("MOVER_ALERT_PCT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5.0),
            milestone_step_idr: std::env::var("MILESTONE_STEP_IDR")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(50_000_000),
        }
    }
}

/// Dedup key when the morning briefing is currently due, else None. Due from
/// the configured hour for GRACE_HOURS; past that the day is forfeited.
pub fn briefing_due(now_wib: DateTime<FixedOffset>, briefing_hour: Option<u32>) -> Option<String> {
    let hour = briefing_hour?;
    let h = now_wib.hour();
    if h >= hour && h < hour + GRACE_HOURS {
        Some(format!("briefing:{}", now_wib.format("%Y-%m-%d")))
    } else {
        None
    }
}

/// Dedup key when the weekly recap is due: Sunday from the configured hour,
/// with grace until Monday 09:00 (keyed to the week that ended on Sunday).
pub fn recap_due(now_wib: DateTime<FixedOffset>, recap_hour: Option<u32>) -> Option<String> {
    let hour = recap_hour?;
    let due = match now_wib.weekday() {
        chrono::Weekday::Sun => now_wib.hour() >= hour,
        chrono::Weekday::Mon => now_wib.hour() < RECAP_MONDAY_GRACE_END_HOUR,
        _ => false,
    };
    if !due {
        return None;
    }
    // On Monday the recapped week is the one that ended yesterday.
    let anchor = if now_wib.weekday() == chrono::Weekday::Mon {
        now_wib - chrono::Duration::days(1)
    } else {
        now_wib
    };
    let week = anchor.iso_week();
    Some(format!("recap:{}-W{:02}", week.year(), week.week()))
}
```

(`TelegramClient`/`Db` imports are used by Task 7's loop; until then they may warn as unused — leave them.)

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::proactive` — expect 4 PASS.

- [ ] **Step 6: Commit**

```bash
git add backend/src/assistant
git commit -m "feat(proactive): add config and due-window scheduling logic"
```

---

### Task 3: Composition with fallback (`compose.rs`)

**Files:**
- Create: `backend/src/assistant/proactive/compose.rs`
- Modify: `backend/src/assistant/proactive/mod.rs` (add `pub mod compose;`)

- [ ] **Step 1: Write failing tests** — create the file:

```rust
//! One LLM call turns a deterministic data block into natural Indonesian
//! prose; any failure falls back to sending the data block itself.

use crate::llm::claude::Part;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_prefixes_the_header() {
        let msg = fallback_message("📋 Briefing (mode ringkas)", "Todo:\n- bayar listrik");
        assert_eq!(msg, "📋 Briefing (mode ringkas)\nTodo:\n- bayar listrik");
    }

    #[test]
    fn prompts_demand_exact_numbers_and_plain_text() {
        for prompt in [BRIEFING_SYSTEM, RECAP_SYSTEM] {
            let lower = prompt.to_lowercase();
            assert!(lower.contains("indonesian"), "{prompt}");
            assert!(lower.contains("exactly"), "{prompt}");
            assert!(lower.contains("no markdown"), "{prompt}");
        }
    }
}
```

- [ ] **Step 2: Run to verify failure**

Add `pub mod compose;` to `backend/src/assistant/proactive/mod.rs`.
Run: `cd backend && cargo test assistant::proactive::compose` — expect COMPILE ERROR.

- [ ] **Step 3: Implement** — insert between imports and tests:

```rust
pub const BRIEFING_SYSTEM: &str = "You write a short daily morning briefing in Indonesian \
for the app owner, delivered over Telegram. Use ONLY the data block provided — copy every \
number exactly as written, never invent or recalculate anything. Plain text only: no Markdown, \
no headers, no **bold**, no tables. At most 15 short lines; use emoji sparingly as bullets. \
Structure: a one-line greeting with the day and date; today's todos and reminders (or say the \
day is clear); a one-or-two-line portfolio summary (net worth, change, notable movers, pending \
reviews when present); remembered facts only if clearly relevant today; one short, grounded \
closing line — no exaggeration. Skip any section whose data is empty.";

pub const RECAP_SYSTEM: &str = "You write a short weekly recap in Indonesian for the app \
owner, delivered over Telegram on Sunday evening. Use ONLY the data block provided — copy \
every number exactly as written, never invent or recalculate anything. Plain text only: no \
Markdown, no headers, no **bold**, no tables. At most 15 short lines; use emoji sparingly. \
Structure: one opening line; productivity (todos done vs created, reminders delivered); the \
week's finances (net worth change, top movers, spending); what's coming next week; one short, \
grounded closing line. Skip any section whose data is empty.";

/// The message sent when the LLM is unavailable or returns nothing usable.
pub fn fallback_message(header: &str, data_block: &str) -> String {
    format!("{header}\n{data_block}")
}

/// Compose prose from the data block, degrading to the plain block on any
/// LLM failure — an ugly briefing beats a missing one.
pub async fn compose(system: &str, data_block: &str, fallback_header: &str) -> String {
    let llm = match crate::llm::claude::ClaudeClient::from_env() {
        Ok(client) => client,
        Err(e) => {
            tracing::warn!("proactive compose: llm unavailable ({e}); using fallback");
            return fallback_message(fallback_header, data_block);
        }
    };
    match llm.complete(system, &[Part::Text(data_block.to_string())]).await {
        Ok(text) if !text.trim().is_empty() => text,
        Ok(_) => {
            tracing::warn!("proactive compose: empty reply; using fallback");
            fallback_message(fallback_header, data_block)
        }
        Err(e) => {
            tracing::warn!("proactive compose failed ({e}); using fallback");
            fallback_message(fallback_header, data_block)
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::proactive::compose` — expect 2 PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant
git commit -m "feat(proactive): add LLM composition with plain-text fallback"
```

---

### Task 4: Alerts (`alerts.rs`)

**Files:**
- Create: `backend/src/assistant/proactive/alerts.rs`
- Modify: `backend/src/assistant/proactive/mod.rs` (add `pub mod alerts;`)

- [ ] **Step 1: Write failing tests** — create the file with imports + tests:

```rust
//! Event-driven financial alerts, evaluated from already-stored data.
//! Pure helpers produce (dedup_key, message) pairs; the tick claims and sends.

use crate::db::Db;
use crate::service::chat::group_id;
use crate::service::movers::Mover;
use rust_decimal::Decimal;

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
```

NOTE for the implementer: check the real `review_item` table columns in `backend/migrations/0002_review_item.sql` before relying on the seed INSERT above — adjust the column list to match the actual schema (the goal is a pending row with a controllable `created_at`). Adjust ONLY the seed helper, not the assertions.

- [ ] **Step 2: Run to verify failure**

Add `pub mod alerts;` to `backend/src/assistant/proactive/mod.rs`.
Run: `cd backend && cargo test assistant::proactive::alerts` — expect COMPILE ERROR.

- [ ] **Step 3: Implement** — insert between imports and tests:

```rust
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
            // one — see super::snapshot_before) vs current net worth.
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
```

NOTE: the `evaluate()` wiring itself is exercised only via Task 7's "doesn't crash on an empty db" loop test; the pure helpers it calls are what this task's tests pin.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::proactive::alerts` — expect 5 PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant
git commit -m "feat(proactive): add financial alert evaluation"
```

---

### Task 5: Morning briefing (`briefing.rs`)

**Files:**
- Create: `backend/src/assistant/proactive/briefing.rs`
- Modify: `backend/src/assistant/proactive/mod.rs` (add `pub mod briefing;`)

- [ ] **Step 1: Write failing tests** — create the file with imports + tests:

```rust
//! Morning briefing: deterministic gathering, then compose-and-send.

use crate::db::Db;
use crate::repo::reminders::ReminderRow;
use crate::repo::todos::TodoRow;
use crate::service::chat::group_id;
use crate::service::movers::Mover;
use rust_decimal::Decimal;

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn data() -> BriefingData {
        BriefingData {
            date_wib: "2026-06-12".into(),
            weekday: "Jumat".into(),
            todos_due_today: vec![],
            todos_overdue: vec![],
            reminders_today: vec![],
            net_worth_idr: dec!(91960083),
            delta_vs_yesterday_idr: Some(dec!(1200000)),
            movers: vec![],
            pending_reviews: 0,
            memory_facts: vec![],
        }
    }

    #[test]
    fn block_contains_date_and_net_worth() {
        let block = render_data_block(&data());
        assert!(block.contains("Jumat, 2026-06-12"), "{block}");
        assert!(block.contains("Rp 91.960.083"), "{block}");
        assert!(block.contains("+Rp 1.200.000"), "{block}");
    }

    #[test]
    fn empty_sections_say_so_instead_of_vanishing() {
        let block = render_data_block(&data());
        // The LLM must be able to see "nothing today" explicitly.
        assert!(block.contains("(tidak ada)"), "{block}");
    }

    #[test]
    fn todos_and_facts_render_with_details() {
        let mut d = data();
        d.todos_due_today = vec![TodoRow {
            id: 3,
            title: "bayar listrik".into(),
            notes: None,
            due_at: Some("2026-06-12T02:00:00Z".into()),
            status: "open".into(),
            created_at: String::new(),
            completed_at: None,
        }];
        d.pending_reviews = 2;
        d.memory_facts = vec![crate::assistant::memory::MemoryFact {
            fact: "gajian tanggal 25".into(),
            valid_at: None,
            name: "R".into(),
        }];
        let block = render_data_block(&d);
        assert!(block.contains("#3 bayar listrik"), "{block}");
        assert!(block.contains("09:00 WIB"), "{block}"); // 02:00Z = 09:00 WIB
        assert!(block.contains("Review pending: 2"), "{block}");
        assert!(block.contains("gajian tanggal 25"), "{block}");
    }

    #[tokio::test]
    async fn gather_works_on_an_empty_db() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let d = gather(&db).await.unwrap();
        assert!(d.todos_due_today.is_empty());
        assert!(d.reminders_today.is_empty());
        assert_eq!(d.pending_reviews, 0);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Add `pub mod briefing;` to `backend/src/assistant/proactive/mod.rs`.
Run: `cd backend && cargo test assistant::proactive::briefing` — expect COMPILE ERROR.

- [ ] **Step 3: Implement** — insert between imports and tests:

```rust
pub struct BriefingData {
    pub date_wib: String,
    pub weekday: String,
    pub todos_due_today: Vec<TodoRow>,
    pub todos_overdue: Vec<TodoRow>,
    pub reminders_today: Vec<ReminderRow>,
    pub net_worth_idr: Decimal,
    pub delta_vs_yesterday_idr: Option<Decimal>,
    pub movers: Vec<Mover>,
    pub pending_reviews: usize,
    pub memory_facts: Vec<crate::assistant::memory::MemoryFact>,
}

/// Indonesian weekday name for the briefing header.
fn weekday_id(day: chrono::Weekday) -> &'static str {
    match day {
        chrono::Weekday::Mon => "Senin",
        chrono::Weekday::Tue => "Selasa",
        chrono::Weekday::Wed => "Rabu",
        chrono::Weekday::Thu => "Kamis",
        chrono::Weekday::Fri => "Jumat",
        chrono::Weekday::Sat => "Sabtu",
        chrono::Weekday::Sun => "Minggu",
    }
}

/// Gather everything deterministically. Each finance source degrades
/// independently; todo/reminder/review failures propagate (they're local DB).
pub async fn gather(db: &Db) -> anyhow::Result<BriefingData> {
    let now_wib = chrono::Utc::now().with_timezone(&crate::assistant::time::wib());
    let today = now_wib.format("%Y-%m-%d").to_string();

    let open_todos = crate::repo::todos::list_open(db).await?;
    let (mut todos_due_today, mut todos_overdue) = (Vec::new(), Vec::new());
    for todo in open_todos {
        let Some(due_at) = &todo.due_at else { continue };
        let due_date_wib = match chrono::DateTime::parse_from_rfc3339(due_at) {
            Ok(dt) => dt.with_timezone(&crate::assistant::time::wib()).format("%Y-%m-%d").to_string(),
            Err(_) => continue,
        };
        if due_date_wib == today {
            todos_due_today.push(todo);
        } else if due_date_wib < today {
            todos_overdue.push(todo);
        }
    }

    let reminders_today = crate::repo::reminders::list_pending(db)
        .await?
        .into_iter()
        .filter(|r| {
            chrono::DateTime::parse_from_rfc3339(&r.remind_at)
                .map(|dt| {
                    dt.with_timezone(&crate::assistant::time::wib()).format("%Y-%m-%d").to_string()
                        == today
                })
                .unwrap_or(false)
        })
        .collect();

    let pending_reviews = crate::repo::review_items::list_by_status(db, "pending")
        .await
        .map(|items| items.len())
        .unwrap_or(0);

    let (net_worth_idr, delta_vs_yesterday_idr) =
        match crate::service::portfolio::build_summary(db).await {
            Ok(summary) => {
                // ~24h baseline: see snapshot_before's doc for why yesterday
                // (snapshot rows are UTC-keyed; "before today" would be a
                // one-hour-old value).
                let yesterday =
                    (now_wib - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
                let delta = match crate::repo::snapshots::history(db).await {
                    Ok(rows) => super::snapshot_before(&rows, &yesterday)
                        .map(|prev| summary.net_worth_idr - prev),
                    Err(_) => None,
                };
                (summary.net_worth_idr, delta)
            }
            Err(e) => {
                tracing::warn!("briefing: portfolio summary unavailable: {e:#}");
                (Decimal::ZERO, None)
            }
        };

    let movers = crate::service::movers::daily_movers(db, 3).await.unwrap_or_else(|e| {
        tracing::warn!("briefing: movers unavailable: {e:#}");
        Vec::new()
    });

    let memory_facts = match crate::assistant::memory::MemoryClient::from_env() {
        Some(client) => {
            client
                .search(&format!("hal penting hari ini {} {today}", weekday_id(now_wib.weekday())), 5)
                .await
        }
        None => Vec::new(),
    };

    Ok(BriefingData {
        date_wib: today,
        weekday: weekday_id(now_wib.weekday()).to_string(),
        todos_due_today,
        todos_overdue,
        reminders_today,
        net_worth_idr,
        delta_vs_yesterday_idr,
        movers,
        pending_reviews,
        memory_facts,
    })
}

fn push_todo_lines(out: &mut String, todos: &[TodoRow]) {
    if todos.is_empty() {
        out.push_str("(tidak ada)\n");
        return;
    }
    for todo in todos {
        out.push_str(&format!("- #{} {}", todo.id, todo.title));
        if let Some(due) = &todo.due_at {
            out.push_str(&format!(" (due {})", crate::assistant::time::to_wib_display(due)));
        }
        out.push('\n');
    }
}

/// Deterministic data block: both the LLM input and the fallback body.
pub fn render_data_block(d: &BriefingData) -> String {
    let mut out = format!("Hari: {}, {} (WIB)\n", d.weekday, d.date_wib);

    out.push_str("Todo jatuh tempo hari ini:\n");
    push_todo_lines(&mut out, &d.todos_due_today);
    out.push_str("Todo terlambat:\n");
    push_todo_lines(&mut out, &d.todos_overdue);

    out.push_str("Reminder hari ini:\n");
    if d.reminders_today.is_empty() {
        out.push_str("(tidak ada)\n");
    } else {
        for r in &d.reminders_today {
            out.push_str(&format!(
                "- {}: {}\n",
                crate::assistant::time::to_wib_display(&r.remind_at),
                r.message
            ));
        }
    }

    out.push_str(&format!("Net worth: Rp {}\n", group_id(&d.net_worth_idr.round_dp(0))));
    if let Some(delta) = &d.delta_vs_yesterday_idr {
        let sign = if delta.is_sign_negative() { "-" } else { "+" };
        out.push_str(&format!("Perubahan vs kemarin: {sign}Rp {}\n", group_id(&delta.abs().round_dp(0))));
    }
    if !d.movers.is_empty() {
        out.push_str("Movers:\n");
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
    out.push_str(&format!("Review pending: {}\n", d.pending_reviews));

    if !d.memory_facts.is_empty() {
        out.push_str("Fakta tersimpan yang mungkin relevan:\n");
        for f in &d.memory_facts {
            out.push_str(&format!("- {}\n", f.fact));
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
        super::compose::BRIEFING_SYSTEM,
        &block,
        "📋 Briefing (mode ringkas)",
    )
    .await;
    client
        .send_message(chat_id, &text)
        .await
        .map_err(|e| anyhow::anyhow!("briefing send failed: {e}"))?;
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::proactive::briefing` — expect 4 PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant
git commit -m "feat(proactive): add morning briefing gathering and rendering"
```

---

### Task 6: Weekly recap (`recap.rs`) + two small repo queries

**Files:**
- Create: `backend/src/assistant/proactive/recap.rs`
- Modify: `backend/src/assistant/proactive/mod.rs` (add `pub mod recap;`)
- Modify: `backend/src/repo/todos.rs` (two new queries + tests)
- Modify: `backend/src/repo/reminders.rs` (one new query + test)

- [ ] **Step 1: Write failing repo tests.**

In `backend/src/repo/todos.rs` tests, add:

```rust
    #[tokio::test]
    async fn completed_since_and_created_count() {
        let db = mem_db().await;
        let a = create(&db, "old done", None, None).await.unwrap();
        complete(&db, a.id).await.unwrap();
        let b = create(&db, "new open", None, None).await.unwrap();
        let _ = b;
        // Everything above happened "now", so a since-bound in the past
        // includes them and a future bound excludes them.
        let past = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
        let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        assert_eq!(completed_since(&db, &past).await.unwrap().len(), 1);
        assert_eq!(completed_since(&db, &future).await.unwrap().len(), 0);
        assert_eq!(created_count_since(&db, &past).await.unwrap(), 2);
        assert_eq!(created_count_since(&db, &future).await.unwrap(), 0);
    }
```

In `backend/src/repo/reminders.rs` tests, add:

```rust
    #[tokio::test]
    async fn sent_count_since_counts_delivered_reminders() {
        let db = mem_db().await;
        let r = create(&db, None, "x", "2026-06-10T00:00:00Z", "none").await.unwrap();
        mark_sent(&db, r.id, "2026-06-10T08:00:00Z").await.unwrap();
        create(&db, None, "pending", "2099-01-01T00:00:00Z", "none").await.unwrap();
        assert_eq!(sent_count_since(&db, "2026-06-09T00:00:00Z").await.unwrap(), 1);
        assert_eq!(sent_count_since(&db, "2026-06-11T00:00:00Z").await.unwrap(), 0);
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test repo::todos repo::reminders` — expect COMPILE ERROR.

- [ ] **Step 3: Implement the repo queries.**

In `backend/src/repo/todos.rs` (note: `created_at`/`completed_at` use `to_rfc3339()` format — pass `since` in the same format):

```rust
/// Done todos whose completion is at/after `since` (RFC3339 +00:00 format,
/// the same format `complete` writes).
pub async fn completed_since(db: &Db, since_rfc3339: &str) -> anyhow::Result<Vec<TodoRow>> {
    let rows = sqlx::query_as::<_, TodoRow>(
        "SELECT * FROM todos WHERE status = 'done' AND completed_at >= ? ORDER BY completed_at",
    )
    .bind(since_rfc3339)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// How many todos were created at/after `since` (RFC3339 +00:00 format).
pub async fn created_count_since(db: &Db, since_rfc3339: &str) -> anyhow::Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM todos WHERE created_at >= ?")
        .bind(since_rfc3339)
        .fetch_one(db)
        .await?;
    Ok(row.0)
}
```

In `backend/src/repo/reminders.rs` (note: `sent_at` uses the Z format written by the reminder tick — pass `since` in Z format):

```rust
/// How many reminders were delivered at/after `since` ("%Y-%m-%dT%H:%M:%SZ",
/// the format the delivery tick writes to sent_at).
pub async fn sent_count_since(db: &Db, since_z: &str) -> anyhow::Result<i64> {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM reminders WHERE sent_at IS NOT NULL AND sent_at >= ?")
            .bind(since_z)
            .fetch_one(db)
            .await?;
    Ok(row.0)
}
```

Run: `cd backend && cargo test repo::todos repo::reminders` — expect all PASS (incl. 2 new).

- [ ] **Step 4: Write failing recap tests** — create `backend/src/assistant/proactive/recap.rs`:

```rust
//! Weekly recap: deterministic gathering, then compose-and-send.

use crate::db::Db;
use crate::service::chat::group_id;
use rust_decimal::Decimal;

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn weekly_spending_sums_idr_outflows_in_window() {
        let rows = vec![
            cf("2026-06-08", "out", "150000", "IDR"),
            cf("2026-06-10", "out", "50000", "IDR"),
            cf("2026-06-10", "in", "9000000", "IDR"),   // income ignored
            cf("2026-06-01", "out", "999999", "IDR"),   // before window
            cf("2026-06-10", "out", "20", "USD"),       // non-IDR skipped
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
}
```

- [ ] **Step 5: Run to verify failure**

Add `pub mod recap;` to `backend/src/assistant/proactive/mod.rs`.
Run: `cd backend && cargo test assistant::proactive::recap` — expect COMPILE ERROR.

- [ ] **Step 6: Implement** — insert between imports and tests:

```rust
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

    let movers = crate::service::movers::daily_movers(db, 3).await.unwrap_or_default();

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
        .filter(|r| r.remind_at.as_str() <= next_week_end.as_str())
        .collect();

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
    })
}

/// Deterministic data block: LLM input and fallback body.
pub fn render_data_block(d: &RecapData) -> String {
    let mut out = format!("Rekap minggu {}\n", d.week_label);
    out.push_str(&format!("Todo selesai: {} (dibuat: {})\n", d.todos_completed, d.todos_created));
    out.push_str(&format!("Reminder terkirim: {}\n", d.reminders_sent));
    out.push_str(&format!("Net worth: Rp {}\n", group_id(&d.net_worth_idr.round_dp(0))));
    if let Some(delta) = &d.week_delta_idr {
        let sign = if delta.is_sign_negative() { "-" } else { "+" };
        out.push_str(&format!(
            "Perubahan seminggu: {sign}Rp {}\n",
            group_id(&delta.abs().round_dp(0))
        ));
    }
    out.push_str(&format!("Pengeluaran minggu ini: Rp {}\n", group_id(&d.spending_idr.round_dp(0))));
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
    out.push_str("Minggu depan:\n");
    if d.todos_next_week.is_empty() && d.reminders_next_week.is_empty() {
        out.push_str("(tidak ada jadwal tercatat)\n");
    } else {
        for t in &d.todos_next_week {
            out.push_str(&format!("- todo #{} {}", t.id, t.title));
            if let Some(due) = &t.due_at {
                out.push_str(&format!(" (due {})", crate::assistant::time::to_wib_display(due)));
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
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::proactive::recap` — expect 3 PASS.

- [ ] **Step 8: Commit**

```bash
git add backend/src/assistant backend/src/repo
git commit -m "feat(proactive): add weekly recap gathering and rendering"
```

---

### Task 7: Tick loop + main wiring

**Files:**
- Modify: `backend/src/assistant/proactive/tick.rs`
- Modify: `backend/src/main.rs` (one spawn line)

- [ ] **Step 1: Write failing test** — add to the tests module in `tick.rs`:

```rust
    #[tokio::test]
    async fn run_once_claims_and_survives_an_empty_db_without_a_client() {
        // With no telegram link, run_once must be a clean no-op.
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let config = ProactiveConfig {
            briefing_hour: Some(0), // "always due" for this test
            recap_hour: Some(0),
            mover_alert_pct: 5.0,
            milestone_step_idr: 50_000_000,
        };
        let client = TelegramClient::new("dummy-token".into());
        run_once(&db, &client, &config).await.unwrap();
        // No link -> nothing claimed.
        assert!(crate::repo::proactive_log::try_claim(&db, "briefing", &format!(
            "briefing:{}",
            chrono::Utc::now().with_timezone(&crate::assistant::time::wib()).format("%Y-%m-%d")
        ))
        .await
        .unwrap());
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test assistant::proactive::tick` — expect COMPILE ERROR (`run_once` not found).

- [ ] **Step 3: Implement** — add to `tick.rs` (below the due-window functions):

```rust
const TICK: std::time::Duration = std::time::Duration::from_secs(300);

/// Spawn the proactive loop when TELEGRAM_BOT_TOKEN is configured.
pub fn spawn(db: Db) {
    let Ok(token) = std::env::var("TELEGRAM_BOT_TOKEN") else {
        tracing::info!("TELEGRAM_BOT_TOKEN not set; proactive sends disabled");
        return;
    };
    tokio::spawn(async move {
        let client = TelegramClient::new(token);
        let config = ProactiveConfig::from_env();
        loop {
            if let Err(e) = run_once(&db, &client, &config).await {
                tracing::warn!("proactive tick failed: {e:#}");
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

/// One pass: claim-then-send for whatever is due. Claiming BEFORE sending
/// makes every send at-most-once (a duplicate briefing annoys more than a
/// missing one — the inverse of the reminder loop's trade-off).
pub async fn run_once(
    db: &Db,
    client: &TelegramClient,
    config: &ProactiveConfig,
) -> anyhow::Result<()> {
    let Some(link) = crate::repo::telegram_link::get(db).await? else {
        return Ok(());
    };
    let now_wib = chrono::Utc::now().with_timezone(&crate::assistant::time::wib());
    let today = now_wib.format("%Y-%m-%d").to_string();

    if let Some(key) = briefing_due(now_wib, config.briefing_hour) {
        if crate::repo::proactive_log::try_claim(db, "briefing", &key).await? {
            if let Err(e) = super::briefing::run(db, client, link.chat_id).await {
                tracing::warn!("briefing for {key} forfeited: {e:#}");
            }
        }
    }

    if let Some(key) = recap_due(now_wib, config.recap_hour) {
        if crate::repo::proactive_log::try_claim(db, "recap", &key).await? {
            if let Err(e) = super::recap::run(db, client, link.chat_id).await {
                tracing::warn!("recap for {key} forfeited: {e:#}");
            }
        }
    }

    for alert in
        super::alerts::evaluate(db, config.mover_alert_pct, config.milestone_step_idr, &today).await
    {
        if crate::repo::proactive_log::try_claim(db, "alert", &alert.dedup_key).await? {
            if let Err(e) = client.send_message(link.chat_id, &alert.message).await {
                tracing::warn!("alert {} forfeited: {e:#}", alert.dedup_key);
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 4: Wire into main.** In `backend/src/main.rs`, after `assistant::reminder_tick::spawn(db.clone());` add:

```rust
    assistant::proactive::tick::spawn(db.clone());
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::proactive` — all proactive tests PASS (expect 13 across the module). `cargo build` — expect no warnings.

- [ ] **Step 6: Commit**

```bash
git add backend/src/assistant backend/src/main.rs
git commit -m "feat(proactive): run briefing, recap, and alerts from a 5-minute tick"
```

---

### Task 8: Full verification + env docs

**Files:**
- Modify: `.env.production.example` (repo root)

- [ ] **Step 1: Document the optional knobs.** Append to `.env.production.example`:

```
# Proactive sends (all optional; defaults shown). Set an hour to "off" to disable.
BRIEFING_HOUR_WIB=7
RECAP_HOUR_WIB=17
MOVER_ALERT_PCT=5
MILESTONE_STEP_IDR=50000000
```

- [ ] **Step 2: Full suites**

Run: `cd backend && cargo test` — expect ~331 passed (314 baseline + ~17 new; trust the measured number), 0 failed. `cargo build` — 0 warnings.

- [ ] **Step 3: Manual smoke (needs real tokens; coordinate with the user)**

1. Run the backend with `BRIEFING_HOUR_WIB` set to the current WIB hour → within 5 minutes a briefing arrives; restart the backend → no duplicate.
2. Temporarily set `MOVER_ALERT_PCT=0.1` → mover alerts arrive once; again, restart produces no duplicates.
3. Unset `ANTHROPIC_API_KEY` and re-trigger a briefing (new day or fresh DB) → the "📋 Briefing (mode ringkas)" fallback arrives instead of silence.

- [ ] **Step 4: Commit**

```bash
git add .env.production.example
git commit -m "docs(deploy): document proactive scheduling env knobs"
```

---

## Self-Review Notes

- **Spec coverage:** dedup table + claim-before-send at-most-once (Tasks 1, 7), due windows with grace + Monday recap grace + "off" switch (Task 2), LLM compose with exact-numbers prompt + fallback (Task 3), all four alerts incl. milestone once-ever and stale-source filter (Task 4), briefing content incl. memory facts via degrading search (Task 5), recap content incl. spending with non-IDR skip note (Task 6), gate on token+link (Task 7), env knobs documented (Task 8). Out-of-scope items have no tasks.
- **Type consistency:** `Alert { dedup_key, message }` (Task 4) consumed in Task 7; `ProactiveConfig` fields match between Tasks 2 and 7; `render_data_block`/`gather`/`run` signatures consistent between briefing and recap; repo additions (`completed_since`, `created_count_since`, `sent_count_since`) match their Task 6 call sites.
- **Known judgment calls:** (1) milestone baseline is the latest snapshot vs live net worth — between snapshot updates the same crossing can only fire once anyway (dedup is per-value, forever). (2) `evaluate()` glue is covered only by the empty-db loop test; its pure helpers carry the real assertions. (3) Briefing memory query is a single semantic search with the date in it; relevance depends on Graphiti's ranking — accepted, worst case the section is mildly irrelevant and the prompt says to skip non-relevant facts.
