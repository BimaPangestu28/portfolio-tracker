# Amount-only mutual fund buys — design

Date: 2026-06-05
Status: approved

## Problem

Screenshot ingestion of Bibit's "Transaksi → Order" tab extracts mutual fund
purchases correctly (fund name, IDR amount, status), but the entries cannot be
confirmed into transactions:

- The Order tab shows no units, NAV, or date, so `quantity`, `price_native`,
  `executed_at`, and `symbol` are null in the extracted entry.
- `txn.quantity` and `txn.price_native` are NOT NULL; confirm fails with
  `bad decimal ''` (`repo/transactions.rs` validation on empty strings).
- `needs_attention()` flags every such entry (buy with null symbol/quantity).
- Instrument matching is exact-symbol-only, so fund names never match.
- The review UI renders the null fields as empty inputs, which reads as a
  failed extraction.

Observed in production: batch `batch-1780641459498` (review items 54–56 from
`asdc.jpeg`) — 3 buy entries with `amount_native` 13.000.000 / 20.000.000 /
3.000.000 IDR, confidence 0.72, everything else null.

## Decisions (user-confirmed)

1. **Value-based forever**: mutual fund positions are tracked by invested IDR
   amount. No unit/NAV backfill or settlement flow — not now, not later.
2. **Goal → account, fund → instrument**: Bibit goals (Pendidikan Noah, Mobil,
   Dana Darurat) map to separate accounts; fund names map to instruments. The
   same fund bought under two goals stays as two positions.
3. **Value updates out of scope**: positions are valued at cost via the
   existing `avg_cost` fallback. Raising value later uses the existing
   `POST /prices/manual` endpoint; no Bibit portfolio-snapshot extraction in
   this change.
4. **Approach A**: represent amount-only buys with the existing convention
   `quantity = amount_native`, `price_native = "1"` (same convention as
   connector deposits in `service/sync.rs`). No schema change.

## Design

### Data flow

```
Screenshot Bibit → POST /ingest → Claude vision
  → ExtractedEntry {
      instrument_name: "Sucorinvest Bond Fund"   ← clean fund name, no goal
      account_hint:    "Pendidikan Noah"          ← Bibit goal
      amount_native:   "13000000", currency: IDR
      quantity / price_native / symbol: null
    }
  → review item (not flagged when name + amount present and confidence ≥ 0.6)
  → confirm → txn { quantity = 13000000, price_native = "1", currency = IDR }
  → cost basis = invested amount; valuation at cost via avg_cost fallback
```

### 1. Extraction prompt (`ingestion/ingest.rs` SYSTEM_PROMPT)

Add rules for Indonesian mutual fund apps (Bibit and similar):

- The goal/tujuan name (e.g. "Pendidikan Noah", "Dana Darurat") goes in
  `account_hint`, never concatenated into `instrument_name`.
- The fund name (e.g. "Sucorinvest Bond Fund") goes in `instrument_name`,
  clean.
- Purchases shown as an IDR amount only: put the total in `amount_native`,
  leave `quantity` and `price_native` null. Never invent units or NAV.
- Skip failed/cancelled orders; put order status (e.g. "Pembelian Berhasil")
  in `note`.

The IDX lot-correction guard in `normalize_entry()` already skips these rows
(it requires a parsed price ≥ 50; price is null here). Verified by test.

### 2. Confirm flow (`ingestion/review.rs`)

- Add `amount_native` to `ConfirmPayload`.
- If `quantity` and `price_native` are both empty/absent, `amount_native` is
  present, and `entry_type ∈ {buy, sell}`: set `quantity = amount_native`,
  `price_native = "1"`.
- Amount-only sell is supported (Bibit redemptions are nominal too). With
  avg cost = 1, realized P&L computes to 0 — consistent with value-based
  tracking.
- If quantity, price, and amount are all missing: return a 400
  `invalid input: quantity/price or amount required` instead of the current
  500 `bad decimal ''`.
- `amount_native` is validated as a decimal before being used as quantity.

### 3. `needs_attention()` (`ingestion/ingest.rs`)

For buy/sell entries, the completeness check becomes:
`(symbol OR instrument_name)` present AND `(quantity OR amount_native)`
present. The confidence < 0.6 trigger and `force_attention` are unchanged.

### 4. Instrument matching (`ingestion/matching.rs`)

`suggest_instrument()` additionally matches `LOWER(name) = LOWER(?)` against
the extracted `instrument_name` when the symbol lookup misses. Exact,
case-insensitive only — no fuzzy/substring matching. Account matching by
name already works via `account_hint` and is unchanged.

### 5. Review UI (`frontend/src/components/ReviewRow.tsx`)

- Show an editable **Amount** field bound to `amount_native`.
- When quantity/price are empty but amount is present, render an
  "amount-only" hint in place of the empty quantity/price inputs; the inputs
  remain available for manual override.
- Pass `amount_native` through in the confirm payload.
- Instrument selection/creation and account selection reuse the existing
  inline flows. No auto-creation of accounts from goal hints (user creates
  each goal-account once, manually).

### Explicitly unchanged

- `txn` schema, `Transaction`/`NewTransaction` types.
- `cost_basis.rs`, `valuation.rs`, `performance.rs`, `insights.rs` — the
  `quantity × price` convention keeps all of them correct.
- CSV ingestion shares `ExtractedEntry` + confirm, so it gains amount-only
  support with no extra work.

## Error handling

- Confirm with no quantity, no price, no amount → 400 with a clear message.
- Malformed `amount_native` → existing decimal validation error path.
- LLM emitting prose around JSON → already handled by `extract_json()`
  (commit 7eb6f7e).

## Testing

- `confirm()`: amount-only buy maps to `quantity=amount, price="1"`;
  amount-only sell; all-missing → 400; quantity+price present → existing
  path byte-identical.
- `needs_attention()`: name+amount → false; low confidence → true;
  stock-style entries unchanged.
- `suggest_instrument()`: case-insensitive name match; symbol match still
  takes precedence.
- Extraction fixture: the exact `raw_llm_json` from production review items
  54–56 parses into 3 entries and `normalize_entry()` applies no lot
  correction.
- Regression: existing Stockbit IDX lot tests stay green.
