# Investment Tracker — Design Spec

**Tanggal:** 2026-05-31
**Status:** Disetujui untuk Fase 1
**Project dir:** `~/Works/portfolio-tracker` (nama bisa diganti)

---

## 1. Problem

Investasi tersebar di banyak aplikasi & instrumen berbeda (crypto exchange, saham IDX,
reksadana, USD ETF, on-chain, dll), sehingga sulit melihat posisi keseluruhan: net worth,
performa, dan apakah alokasi masih sesuai target. Tujuan: satu tempat yang
mengonsolidasikan semuanya secara akurat.

## 2. Visi penuh & dekomposisi

Sistem akhir terdiri dari 6 modul. Karena kegedean untuk satu siklus implementasi, dipecah
jadi fase — tiap fase punya spec → plan → implementasi sendiri.

| Modul | Isi | Fase |
|-------|-----|------|
| 1. Domain core | Accounts, Instruments, **Transaction ledger**, cost-basis engine, Positions, Valuation, **Allocation planner** | **1** |
| 2. Pricing service | Harga terkini & historis: CoinGecko (crypto), Yahoo `.JK` (saham IDX), NAV reksadana, FX USD/IDR | **1** |
| 6. API + Web dashboard | axum REST + React: net worth, performance, allocation vs target, history | **1** |
| 3. Connectors auto-sync | Read-only API exchange (Binance/Indodax) + on-chain wallet (EVM/Solana) → ledger; scheduler | 2 |
| 4. LLM ingestion + budgeting | OCR screenshot, parsing statement bank, CSV importer → **review queue** → ledger; kategori cashflow/budgeting | 3 |
| 5. Chatbot channel-agnostic | Agent LLM (Claude) baca/tulis domain; channel WhatsApp + chat panel in-app | 4 |

**Dokumen ini hanya men-spec Fase 1.** Fase berikutnya di-brainstorm terpisah saat tiba waktunya.

### Prinsip lintas-fase
- **Ledger adalah satu-satunya sumber kebenaran.** Manual entry, CSV, auto-sync, dan hasil
  OCR semuanya dinormalisasi jadi transaksi di ledger yang sama.
- **Hasil ekstraksi LLM tidak pernah langsung commit** — selalu lewat review queue (Fase 3).
- **Tidak ada angka finansial yang gagal secara diam-diam.** Harga gagal di-fetch → tampilkan
  harga terakhir + indikator basi (stale), bukan 0 atau kosong tanpa keterangan.

---

## 3. Scope Fase 1

### In scope
- Manajemen Accounts, Instruments, Categories (manual).
- Transaction ledger dengan tipe: `buy`, `sell`, `dividend`, `interest`, `fee`,
  `deposit`, `withdrawal`, `opening_balance`.
- Cost-basis engine **average cost**; realized & unrealized P&L.
- Valuation dual currency **IDR + USD** (konversi via FX).
- Pricing service: CoinGecko, Yahoo Finance, FX USD/IDR. NAV reksadana **input manual** di Fase 1.
- Dashboard: net worth konsolidasi, performance (ROI + **XIRR**), allocation **vs target + drift**, history (grafik nilai portfolio dari waktu ke waktu).
- Allocation planner: kategori user-defined dengan target % + tolerance band; tampilan target vs aktual + hint rebalancing.

### Out of scope (fase lain)
Auto-sync exchange/on-chain · OCR/screenshot · parsing statement bank · CSV import · budgeting
pengeluaran harian · chatbot/WhatsApp · multi-user/auth kompleks · enkripsi API key (belum ada API key di Fase 1).

---

## 4. Arsitektur Fase 1

```
React/TS (Vite, strict, zod)
        │  REST (JSON)
        ▼
axum API  ──►  Domain core (Rust)         ──►  SQLite (sqlx)
                 ├─ ledger + cost-basis
                 ├─ valuation (dual currency)
                 └─ allocation planner
                       ▲
            Pricing service (trait PriceProvider)
              ├─ CoinGecko   ├─ Yahoo Finance   └─ FX USD/IDR
                       ▲
            Scheduler (refresh harga + snapshot valuasi harian)
```

- **Storage:** SQLite via `sqlx` — pas untuk single-user self-host, satu file, backup gampang.
- **Backend:** Rust, `axum` + `sqlx`, error handling pakai `thiserror` (domain) + `anyhow`
  (boundary). **Tanpa `unwrap()`/`panic!()` di jalur produksi.**
- **Frontend:** TypeScript strict, React + Vite, validasi `zod` di boundary API.
- **Scheduler:** tugas terjadwal (`tokio` task / cron) untuk refresh harga & menyimpan snapshot
  valuasi harian (sumber data grafik history).

---

## 5. Data model

### Account
`id, name, type(exchange|broker|bank|wallet|manual), institution, native_currency, note, created_at`

### Category (allocation planner)
`id, name, target_pct, tolerance_band_pct (nullable), sort_order, color`
- Total `target_pct` semua kategori divalidasi mendekati 100% (warning kalau tidak).

### Instrument
`id, symbol, name, type(crypto|stock_id|stock_us|etf|mutual_fund|cash|bond|gold|other),
native_currency, category_id (FK), price_source (provider+external id, mis. "coingecko:bitcoin",
"yahoo:BBCA.JK", atau "manual"), decimals, note`

### Transaction (ledger)
`id, account_id (FK), instrument_id (FK), type, executed_at, quantity, price_native, fee_native,
currency, fx_to_idr, fx_to_usd, note, created_at`
- `fx_to_idr`/`fx_to_usd` di-snapshot saat transaksi (kurs historis), supaya nilai historis konsisten.
- `opening_balance`: set qty + avg cost awal tanpa efek cashflow (fallback holding legacy tanpa history).

### PriceQuote (cache)
`instrument_id, as_of (date), price_native, currency, source, kind(latest|historical)`

### FxRate
`as_of (date), base, quote, rate` — minimal pasangan USD/IDR.

### ValuationSnapshot (sumber grafik history)
`as_of (date), total_idr, total_usd, breakdown_json (per kategori/instrumen)` — diisi scheduler harian.

### Position (turunan, tidak disimpan; dihitung dari ledger)
`instrument_id, quantity, avg_cost_native, cost_basis_total, market_value_native,
market_value_idr, market_value_usd, unrealized_pnl, realized_pnl, weight_pct`

---

## 6. Logika inti

### Cost basis — average cost
- **Buy:** `new_avg = (old_qty*old_avg + buy_qty*buy_price + fee) / (old_qty + buy_qty)`; qty bertambah.
- **Sell:** `realized += (sell_price - avg)*sell_qty - fee`; qty berkurang, avg tetap.
- **Dividend/interest:** income (cashflow), tidak mengubah qty (reinvest = transaksi `buy` terpisah).
- **opening_balance:** set qty & avg langsung, tidak masuk perhitungan net-invested cashflow XIRR
  kecuali ditandai sebagai modal awal.

### Valuation (dual currency)
`market_value_native = qty * latest_price`; konversi ke IDR & USD pakai FX terkini.
Net worth = Σ posisi dalam IDR dan USD. Tampilkan indikator **stale** bila harga/FX lebih lama dari ambang.

### Performance
- **Unrealized P&L** = market_value − cost_basis.
- **Realized P&L** = akumulasi dari sell.
- **Simple ROI** = (nilai_sekarang + realized + income − net_invested) / net_invested.
- **XIRR** = annualized return berbasis arus kas bertanggal (deposit/buy = outflow; sell/dividend/
  nilai_sekarang = inflow), solusi Newton-Raphson. Ini metrik performa utama yang akurat.

### Allocation planner
Aktual per kategori = Σ market_value posisi di kategori / total. Bandingkan ke `target_pct`;
flag jika |aktual − target| > `tolerance_band_pct`. Hint rebalancing = selisih nominal ke target.

### History
Scheduler menyimpan `ValuationSnapshot` harian. Grafik history membaca deret snapshot.
Backfill awal: tarik harga historis dari provider (CoinGecko/Yahoo) untuk merekonstruksi nilai
dari tanggal transaksi pertama bila memungkinkan.

---

## 7. Pricing service

Trait:
```rust
trait PriceProvider {
    async fn latest(&self, ext_id: &str) -> Result<Quote, PriceError>;
    async fn historical(&self, ext_id: &str, range: DateRange) -> Result<Vec<Quote>, PriceError>;
}
```
Implementasi Fase 1: `CoinGeckoProvider`, `YahooFinanceProvider`, `FxProvider` (USD/IDR).
`mutual_fund` (reksadana) & instrumen ber-`price_source = "manual"`: harga di-input manual,
disimpan sebagai `PriceQuote`.

Kegagalan fetch: log + pakai quote terakhir + tandai stale. **Tidak** menggantikan dengan 0.

---

## 8. API (axum, REST/JSON)

- `GET/POST/PUT/DELETE /accounts`, `/instruments`, `/categories`, `/transactions`
- `GET /portfolio/summary` → net worth (IDR+USD), daftar posisi, allocation vs target, metrik performa
- `GET /portfolio/history?range=` → deret ValuationSnapshot
- `POST /prices/manual` → input harga/NAV manual
- `POST /prices/refresh` → trigger refresh manual (selain scheduler)

Validasi input di boundary; error response terstruktur (kode + pesan), tidak menelan error.

---

## 9. Frontend (React + TS)

Halaman:
- **Dashboard:** kartu net worth dual currency; donut allocation vs target dengan bar drift;
  kartu performa (ROI, XIRR, unrealized, realized); grafik garis history.
- **Holdings:** tabel posisi (qty, avg cost, market value, P&L, weight).
- **Transactions:** CRUD ledger.
- **Planner:** atur kategori, target %, tolerance band; lihat target vs aktual.
- **Settings:** accounts, instruments, input harga manual.

---

## 10. Error handling & testing

- **Error handling:** `thiserror` untuk error domain, `anyhow` di boundary. Tanpa `unwrap`/`panic`
  di produksi. Kegagalan harga/FX terdegradasi anggun + indikator stale, tidak diam-diam.
- **Testing (TDD):**
  - Unit: cost-basis engine (rangkaian buy/sell/dividend), XIRR, valuation dual currency, drift allocation.
  - Integrasi: endpoint API (CRUD + summary + history) terhadap SQLite test.
  - Frontend: validasi zod + komponen kalkulasi ringkasan.

---

## 11. Keputusan & default

| Topik | Keputusan |
|-------|-----------|
| Arsitektur data | Transaction ledger (A) + opening-balance snapshot |
| Cost basis | Average cost |
| Base currency | Dual IDR + USD |
| DB | SQLite (sqlx) |
| Reksadana NAV (Fase 1) | Input manual |
| Performa utama | XIRR (+ ROI sederhana, realized/unrealized) |
| Sumber history | ValuationSnapshot harian via scheduler + backfill historis bila tersedia |

## 12. Risiko & catatan

- **Sumber harga gratis bisa rate-limit / berubah** (CoinGecko, Yahoo unofficial). Mitigasi: cache,
  refresh terjadwal hemat, abstraksi `PriceProvider` agar gampang ganti sumber.
- **FX historis** untuk valuasi historis dual-currency perlu disnapshot per transaksi agar konsisten.
- **NAV reksadana manual** menambah beban input; sumber otomatis dievaluasi di fase lanjutan.
