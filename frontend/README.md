# Portfolio Tracker — Frontend

Vite + React + TypeScript dashboard for the Phase 1A backend, styled with shadcn/ui (sidebar layout, light/dark theme).

## Dev
1. Start the backend: `cd ../backend && cargo run` (binds http://localhost:8081; override with `BIND_ADDR`).
2. Start the frontend: `npm install && npm run dev` (Vite serves http://localhost:5173, proxies `/api` → `:8081`).

## Test
`npm test` (Vitest + Testing Library + MSW).

## Build
`npm run build` → static assets in `dist/`.

## API base
Defaults to `/api` (proxied in dev). Override with `VITE_API_BASE` for production.

## Import (LLM ingestion)
The Import page uploads screenshots/PDFs to `POST /ingest`; the backend (Phase 3A) calls Claude
to extract candidate entries into a review queue. Review, edit, map/create instrument+account,
then confirm (writes to the ledger) or reject. Requires `ANTHROPIC_API_KEY` set for the backend.

## Budget / CSV (Phase 3B)

### Budget page (`/budget`)
Monthly cashflow tracker with Income / Expense / Net stat cards, per-category budget-vs-actual
progress bars (red when over budget), a cashflow entry form, a budget category form, and a
recent entries list with delete. Backed by `GET /cashflow`, `GET /cashflow/categories`, and
`GET /cashflow/summary?month=YYYY-MM`.

### CSV import (Import page)
The Import page includes a "Or import a CSV" section. Paste or upload a `.csv` file, then use
the field-to-column mapping UI to match CSV headers to recognised fields (`entry_type`, `symbol`,
`quantity`, `price_native`, `fee_native`, `currency`, `executed_at`, `account_hint`). An
`entry_type_const` input supplies a constant entry type when there is no mapped column. Clicking
"Import CSV" calls `POST /ingest/csv`, which stages all rows into the review queue — the same
confirm/reject workflow used for LLM-extracted entries applies. Note: the CSV parser assumes
simple comma-separated values with no quoted-comma support.

## Connectors (Phase 2)

### Connectors page (`/connectors`)
Auto-sync connectors that pull on-chain or exchange transactions into the ledger without manual
CSV exports. Currently supports **EVM wallet** connectors (public explorer API e.g. Etherscan):
supply a wallet address, label, optional explorer base URL, and optional API key. On sync the
backend fetches all native transfers and ERC-20 token transfers for the wallet, deduplicates them
via `(source, external_id)`, and inserts recognised instruments directly into the ledger. Tokens
whose symbol cannot be matched to a known instrument are **staged to the Import review queue**
(visible on the Import page) for manual confirmation — the same confirm/reject workflow applies.

The Connectors page shows each connector's kind, label, last-synced timestamp, and provides a
"Sync now" button that runs the sync immediately and displays the resulting
`{inserted, staged, skipped}` count inline.

Exchange connectors (Binance, etc.) are trait-ready but the live implementation is deferred to a
follow-up; they will appear in the same UI once wired up.
