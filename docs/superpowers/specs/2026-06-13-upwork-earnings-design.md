# Upwork Earnings → Cashflow Income — Design

**Date:** 2026-06-13
**Status:** Approved (design); pending implementation plan
**Scope:** Sub-project 1 of a larger Upwork integration. This spec covers **only** ingesting
Upwork earnings into the existing cashflow ledger.

---

## 1. Purpose

Bring the money the user earns on Upwork into the portfolio-tracker as **income**, so it shows up
in the existing cashflow views (and IDR display) — **without** touching the investment portfolio.

The user's key constraint: **Upwork earnings are not portfolio assets.** Earnings are income; only
money the user *deliberately* allocates later becomes a portfolio asset. That allocation step is
**out of scope** for this sub-project.

### Non-goals (explicit, v1)
- Allocating earnings → portfolio (cashflow → investment transaction bridge).
- Upwork fees / withdrawals (Approach B — full ledger mirror).
- Automatic periodic sync (v1 is manual-trigger only).
- Any job-feed, invitation, proposal-draft, or ClickUp feature (separate sub-projects).

---

## 2. Context

The backend already separates two domains, and this design respects that boundary:

- **Portfolio (net worth):** `account`, `instrument`, `transaction`, `snapshot`, plus
  `domain/{cost_basis,valuation,xirr,allocation}`. Fed by `connectors/` (the `ExternalTxn` trait,
  for crypto/investment txns).
- **Cashflow (income/expense):** `cashflow` + `cashflow_category` (migration 0003). Has
  `direction` (`in`/`out`), `amount`, `currency`, `category_id`.

**Upwork earnings belong in the cashflow domain**, as income (`direction = 'in'`). They do **not**
use the `connectors/ExternalTxn` trait — that path feeds portfolio transactions.

The integration mirrors the established **`google/`** module pattern (OAuth + mockable client +
pure reconciler + executor engine + single-row encrypted token store), not the `connectors/`
pattern.

---

## 3. Decision: Approach A — net earnings only

Upwork exposes a transaction ledger (earnings, service fees, withdrawals, bonuses, refunds). This
design records **only earning-type transactions** as cashflow income. Fees and withdrawals are
skipped (logged, not errored).

- ✅ Matches "how much did I earn"; no double-counting when money later lands in a bank account.
- ⚠️ Upwork fee deductions are not visible — acceptable for v1; `external_ref` keeps the door open
  to extend to Approach B (add fee `out` entries) later without rework.

---

## 4. Module layout — `backend/src/upwork/` (mirrors `google/`)

| File | Responsibility |
|---|---|
| `mod.rs` | Module declarations + shared types (`UpworkTransaction`). |
| `oauth.rs` | OAuth2: build consent URL, exchange `code` → tokens, refresh access token. Mirrors `google/oauth.rs`. |
| `crypto.rs` | AES-256-GCM token encryption at rest; key from `UPWORK_TOKEN_ENC_KEY`. Reuses the `google::crypto` primitives (`encrypt`/`decrypt` take a key arg) with an Upwork-specific key loader. Fail-closed on missing/invalid key. |
| `client.rs` | Trait `UpworkClient { async fn fetch_transactions(&self, cursor: Option<&str>) -> Result<TransactionBatch> }`. Real impl calls the Upwork GraphQL API; a mock impl returns fixtures. **The mock enables building and testing before the API key is approved.** |
| `sync.rs` | **Pure reconciler** (no DB, no network): `Vec<UpworkTransaction>` → `Vec<NewCashflow>`, filtered to earning types (Approach A), each carrying `source = "upwork"` + `external_ref`. |
| `engine.rs` | Executor: load integration row → refresh token if expired → call `UpworkClient` → run `sync.rs` planner → idempotent-insert into `cashflow` repo → advance `earnings_cursor`. Records `last_error` on failure. Mirrors `google/engine.rs`. |

### `UpworkTransaction` (decoded from GraphQL)
```
external_id: String   // Upwork transaction reference — idempotency key
date: String          // rfc3339 / YYYY-MM-DD → occurred_on
kind: String          // raw Upwork type/description, used to classify earning vs fee/withdrawal
amount: String        // money stays a string
currency: String      // "USD"
contract: Option<String>  // project/contract name → cashflow.note
```

---

## 5. Persistence — migration `0015_upwork.sql`

1. **`upwork_integration`** — single-row table (mirror `google_integration`):
   - `id` (always 1), `access_token` (encrypted), `refresh_token` (encrypted), `expiry`, `scope`,
     `status`, `last_error`, **`earnings_cursor`**, `created_at`, `updated_at`.
2. **Extend `cashflow`** with two nullable columns:
   - `source TEXT` — e.g. `'upwork'`, `'manual'` (existing rows = NULL/`'manual'`).
   - `external_ref TEXT` — the Upwork transaction id.
   - **Unique index `(source, external_ref)`** (partial: where `source IS NOT NULL`) — the
     idempotency guarantee; re-syncing the same transaction is a no-op.
3. **"Upwork" cashflow category** — ensured by name at sync time (kind = income), not hard-coded
   by id, so it self-heals across environments.

> Migration-number check: confirm `0015` is free vs `origin/main` before merging (see project
> memory on sqlx migration collisions).

---

## 6. Mapping (Approach A)

For each `UpworkTransaction` whose `kind` is in the earning allowlist
(fixed-price milestone release, hourly charge, bonus):

```
NewCashflow {
  account_id: None,
  occurred_on: txn.date,
  direction: "in",
  amount: txn.amount,
  currency: "USD",          // stored natively; IDR is display-only via pricing/ + formatUSD/IDR
  category_id: <Upwork category id>,
  note: txn.contract,
  // persisted alongside via the extended columns:
  source: "upwork",
  external_ref: txn.external_id,
}
```

- Non-earning kinds (fee, withdrawal, refund, unknown) → **skipped + logged**, never an error.
- Insert guarded by the `(source, external_ref)` unique index → idempotent.
- **Portfolio tables are never written.**

---

## 7. API routes — `api/upwork.rs` (mirror `api/google.rs`)

| Method + path | Purpose |
|---|---|
| `GET /api/upwork/oauth/start` | Return the Upwork consent URL (frontend redirects browser). |
| `GET /api/upwork/oauth/callback` | Exchange `code`, store encrypted tokens, mark connected. Guarded by signed `state` (CSRF), mirroring google. |
| `GET /api/upwork/status` | Connection status: connected / last sync / last_error. |
| `POST /api/upwork/sync` | Trigger an earnings sync now (manual). |
| `POST /api/upwork/disconnect` | Clear the integration row. |

### Environment variables (mirror google wiring in compose + k8s)
- `UPWORK_CLIENT_ID`
- `UPWORK_CLIENT_SECRET`
- `UPWORK_REDIRECT_URI` (e.g. `https://<domain>/api/upwork/oauth/callback`)
- `UPWORK_TOKEN_ENC_KEY` (base64 32 bytes)

---

## 8. Error handling

- Missing/invalid `UPWORK_TOKEN_ENC_KEY` → **fail closed** ("cannot connect"); never store
  plaintext tokens.
- `401` from Upwork → refresh the access token once and retry; if refresh fails, set
  `status = 'error'` + `last_error`, surfaced via `/status`.
- Unknown/non-earning transaction kinds → skipped + logged, sync still succeeds.
- Idempotent upsert means a partial/retried sync never duplicates income.

---

## 9. Testing (TDD)

- **`sync.rs` planner** — table-driven: a fixture batch mixing earnings, fees, withdrawals, and an
  unknown kind → asserts only earnings map, `external_ref`/`note` preserved, `currency == "USD"`,
  `direction == "in"`.
- **`crypto.rs`** — encrypt/decrypt round-trip; missing key fails closed.
- **`oauth.rs`** — consent URL construction + token-response parsing (mirror google oauth tests).
- **`engine.rs` + mock `UpworkClient`** — against an in-memory DB: asserts cashflow rows created,
  **idempotency** (second run inserts nothing new), and `earnings_cursor` advances.
- **Gated live smoke test** — behind an env flag, skipped by default (mirror the existing gated
  google calendar smoke test).

---

## 10. Frontend (minimal, additive)

- A **"Connect Upwork"** card + status indicator on the Connectors page, mirroring the existing
  Google card. Additive only — does not alter `src/api/{client,schemas,hooks}.ts` semantics.
- Upwork income appears automatically in existing cashflow views (no new view needed for v1).
- Richer UI (per-project breakdown, etc.) is deferred.

---

## 11. Build-before-key plan

The Upwork API key is disabled by default and approval takes ~2 weeks. The mockable `UpworkClient`
lets the entire module — schema, OAuth scaffolding, reconciler, engine, tests — be built and
verified now against fixtures. When the real key is approved, only the live `UpworkClient` impl and
the env credentials are swapped in; no design change.
