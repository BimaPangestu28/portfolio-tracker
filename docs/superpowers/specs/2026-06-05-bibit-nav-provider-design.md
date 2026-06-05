# Bibit NAV provider + unit derivation at confirm — design

Date: 2026-06-05
Status: approved

## Problem

Amount-only mutual fund buys (spec `2026-06-05-amount-only-mutual-fund-buys-design.md`)
are recorded as `quantity = amount, price = 1` and valued at cost forever — no
NAV feed existed. Research (2026-06-05) found one: Bibit's public product pages
embed full fund data as JSON in a `<script id="__NEXT_DATA__">` blob — no auth,
daily T-1 updates. Verified live:

- `https://bibit.id/reksadana/RD1436/...` → Sucorinvest Bond Fund, nav 1697.22 @ 2026-06-04
- `https://bibit.id/reksadana/RD831/...` → Majoris Pasar Uang Indonesia, nav 1617.0896 @ 2026-06-04

Path: `props.pageProps.productDetail.nav.{value,date}` (plus `name`, `aum`, etc.).

With NAV accessible, the earlier "value-based forever" decision is revised:
fund positions get real units and a daily-moving market value.

## Decisions (user-confirmed)

1. **Real units**: at confirm time, amount-only fund buys derive
   `quantity = amount / NAV` (4 dp), `price_native = NAV`.
2. **Confirm never fetches live**: it reads the latest stored quote. No quote →
   fall back to the existing `quantity = amount, price = 1` convention, with a
   note. Confirm never fails because Bibit is down.
3. **RDCODE is manual**: `instrument.price_source = "bibit:RD1436"` — same
   pattern as `"yahoo:BBCA.JK"`. No name→code auto-resolution.
4. **Staleness window for fund quotes is 6 days / 144h** (NAV is T-1 with
   date-only as_of at midnight UTC and pauses on weekends/market holidays; a
   single Monday exchange holiday makes Friday's NAV ~107h old before Tuesday's
   refresh — 4 days / 96h was too tight). Week-long closures (Lebaran) will
   still flag stale — by design, the data really is old. Other sources keep the
   existing 24h window.

## Design

### 1. Provider — `backend/src/pricing/bibit.rs` (new)

`BibitClient` with `async fn latest(&self, code: &str) -> Result<NavQuote, PriceError>`:

- `GET https://bibit.id/reksadana/{code}/x` with a desktop `User-Agent`
  (any slug works — the server 307-redirects to the canonical slug; reqwest follows redirects by default).
- Locate the `<script id="__NEXT_DATA__" type="application/json">...</script>`
  payload with plain string search (no HTML parser dependency), parse with
  `serde_json`, and read `props.pageProps.productDetail.nav.value` (number)
  and `.nav.date` (string date).
- Returns `NavQuote { price: Decimal, as_of: String }`; currency is always IDR.
- Failure mapping: transport → `PriceError::Http`; HTTP non-200 → `Http`;
  missing script tag, JSON error, or missing/invalid `nav` → `PriceError::Parse`
  with a message naming what was missing. No panics, no `unwrap()`.
- JSON extraction and `nav` parsing live in a pure function
  (`parse_nav(html: &str) -> Result<NavQuote, PriceError>`) unit-tested against
  a trimmed fixture captured from the real RD1436 page.

### 2. Dispatch — `backend/src/pricing/service.rs`

New branch in `refresh_all`, following the gold/yahoo pattern:

```rust
if let Some(code) = ins.price_source.strip_prefix("bibit:") {
    match bibit.latest(code).await {
        Ok(q) => prices::upsert_latest(db, ins.id, q.price, "IDR", "bibit", &q.as_of).await?,
        Err(e) => tracing::warn!("bibit nav fetch failed for {}: {e}", ins.symbol),
    }
}
```

- **`as_of` is the NAV date from the page, not today** — keeps staleness honest
  and avoids storing the same T-1 NAV under multiple dates.
- One fund failing logs a warning and does not abort the rest of the refresh
  (existing per-instrument error pattern).
- The existing daily scheduler triggers it; `POST /prices/refresh` covers
  on-demand (e.g. right after creating a new fund instrument).

### 3. Source-aware staleness — `backend/src/service/portfolio.rs`

The stale check (currently >24h) becomes source-aware: quotes with
`source == "bibit"` use a 6-day / 144h window; everything else keeps 24h.
`LatestPrice` already carries `source`, so no repo change.

### 4. Unit derivation — `backend/src/ingestion/review.rs::confirm()`

Inside the existing amount-only gate (both quantity and price empty,
`amount_native` present, entry buy/sell), before the price=1 fallback:

1. Load the instrument (already fetched for validation) and check
   `price_source.starts_with("bibit:")`.
2. **Consistency gate**: before attempting NAV derivation, call
   `transactions::has_price_one_txn(db, instrument_id)`. If the instrument
   already has any ledger row with `price_native = '1'` (the value-based
   convention), skip derivation and record `quantity = amount, price = 1`
   with note `"(dicatat nominal di harga 1 agar konsisten dengan transaksi
   sebelumnya)"`. This prevents a mixed-convention position where a
   NAV-derived sell of ~3,000 units could never close a 13,000,000-"unit"
   price-1 buy. To unlock derivation for an instrument, edit the legacy rows
   to real units first.
3. If so (and the consistency gate passes), read `prices::latest(db,
   instrument_id)`. The stored quote is only treated as NAV when
   `lp.source == "bibit"` (belt-and-suspenders: a stray manual or yahoo
   quote must not be mistaken for NAV). If a bibit quote exists with a
   positive price: `quantity = (amount / nav).round_dp(4)`,
   `price_native = nav`, and append to the note:
   `"(unit dihitung dari NAV <nav> per <as_of>)"`.
4. Otherwise (non-bibit instrument, no quote yet, or quote from non-bibit
   source): existing behavior — `quantity = amount, price = "1"`, note
   appended `"(NAV belum tersedia; dicatat nominal di harga 1)"` for bibit
   instruments.

Derived values still flow through `repo::dec()` validation in
`transactions::create`. Telegram one-tap confirm uses the same `confirm()`,
so it gains derivation with zero telegram changes. Amount-only sells derive
the same way, making realized P&L real (NAV vs avg cost).

### 5. Per-fund setup (manual, once)

Create each fund instrument with `price_source = "bibit:RDxxxx"`,
`instrument_type = "mutual_fund"`, `native_currency = "IDR"`, `decimals = 4`.
No migration: no fund transactions have been confirmed with the price=1
convention in production (items 54–56 were rejected).

### 6. Frontend (text only) — `frontend/src/pages/ImportPage.tsx`

The amount-only hint becomes: `"amount-only — unit dihitung otomatis dari NAV
(tanpa NAV: dicatat nominal di harga 1)"`. No logic changes.

### Explicitly unchanged

- `txn` schema, cost basis, valuation math, XIRR, insights.
- The amount-only confirm gate and error message from the previous spec.
- CSV ingestion, matching, extraction prompt.

## Error handling

- Provider: every failure is a typed `PriceError`; refresh logs and skips.
- Confirm: no network calls; DB-read only; fallback never errors.
- A NAV of zero or a non-positive parsed value is treated as `Parse` failure
  (never divide by zero or store a zero quote).

## Testing

- `parse_nav()`: real-page fixture → correct price/date; fixture with the
  `nav` field removed → `Parse`; garbage HTML → `Parse`.
- `confirm()` derivation: bibit instrument + stored quote → derived units
  (assert 4 dp rounding and exact NAV price); bibit instrument without quote →
  price=1 fallback + note; non-bibit instrument → unchanged behavior
  (existing tests stay green).
- Staleness: bibit-source quote aged ~4d11h (Friday NAV, Monday holiday,
  Tuesday refresh) → not stale; aged 7 days → stale; yahoo-source quote aged
  2 days → stale. Boundary: exactly 144h is fresh, 144h+1s is stale.
- Dispatch: unit test for the price/`as_of` mapping if testable without
  network; otherwise covered by provider + repo tests (no live-network tests
  in CI).
