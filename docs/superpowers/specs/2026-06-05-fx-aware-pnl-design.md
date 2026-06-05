# FX-aware P&L + penghapusan toggle currency top bar

**Tanggal:** 2026-06-05
**Status:** Disetujui

## Masalah

1. Toggle IDR/USD di top bar (`frontend/src/components/AppShell.tsx`) adalah dead code: state disimpan ke localStorage + `CurrencyContext`, tapi tidak ada komponen yang consume `useCurrency()`. PerformancePage punya toggle lokal sendiri.
2. P&L untuk aset berdenominasi USD tidak menangkap FX gain/loss. Formula sekarang:
   `unrealized_pnl_idr = (mv_native − cost_basis_native) × kurs_sekarang`
   Pergerakan kurs IDR/USD terhadap pokok modal (principal) hilang. Contoh: beli $100 saat kurs 16.000, harga tetap $100, kurs naik ke 17.000 → tampilan P&L Rp 0, padahal nilai riil naik Rp 100.000.

Data untuk memperbaiki ini sudah ada: `txn.fx_to_idr` (kurs saat transaksi) tersimpan per transaksi.

## Keputusan desain

- P&L IDR dipecah dua komponen: **price P&L** (pergerakan harga aset) dan **FX P&L** (pergerakan kurs native→IDR).
- Berlaku untuk **unrealized dan realized**.
- Breakdown muncul di: Dashboard summary cards, Holdings table per-row, PerformancePage.
- Pendekatan: **dual-currency cost basis di engine** (eksak untuk FIFO/partial sell), bukan aproksimasi post-hoc.
- Snapshot harian menyimpan dekomposisi mulai sekarang (kolom baru, nullable); historis lama tidak direkonstruksi.
- Toggle top bar + `CurrencyContext`/`useCurrency` dihapus.

## Formula

Definisi komponen (native = currency instrumen, mis. USD):

```
total P&L (IDR) = mv_native × kurs_now − cost_basis_idr
price P&L (IDR) = (mv_native − cost_basis_native) × kurs_now
FX P&L (IDR)    = total − price
                = cost_basis_native × kurs_now − cost_basis_idr
```

Invariant: `price + fx = total`, selalu eksak (FX adalah residual).

Realized saat sell (per lot yang dikonsumsi):

```
realized_idr       = proceeds_native × fx_jual − cost_idr_terkonsumsi
realized_price_idr = realized_native × fx_jual
realized_fx_idr    = realized_idr − realized_price_idr
```

Instrumen IDR: `fx_to_idr = 1` → FX P&L = 0 secara natural, tanpa special case.

## Komponen

### 1. Backend — cost basis engine (`backend/src/domain/cost_basis.rs`)

- Tiap lot menyimpan `cost_native` (existing) dan `cost_idr = (qty × price_native + fee_native) × fx_to_idr` dari transaksi beli.
- Konsumsi lot saat sell tidak berubah (FIFO/avg seperti sekarang); sekalian menghitung `realized_idr`, `realized_price_idr`, `realized_fx_idr` kumulatif.
- Output engine bertambah: `cost_basis_idr_total`, `realized_pnl_idr`, `realized_price_pnl_idr`, `realized_fx_pnl_idr`.

### 2. Backend — valuation & summary (`backend/src/domain/valuation.rs`, `backend/src/service/portfolio.rs`)

- `Position` field baru: `unrealized_pnl_idr`, `unrealized_price_pnl_idr`, `unrealized_fx_pnl_idr`, plus field realized di atas.
- `PortfolioSummary` agregasi ketiga komponen (unrealized & realized).
- API response (summary + holdings) bertambah field — additive, backward compatible.

### 3. Backend — snapshot

- Migration baru: kolom `price_pnl_idr`, `fx_pnl_idr` (nullable) di tabel snapshot.
- Job snapshot harian menulis dekomposisi mulai sekarang. Baris lama NULL.

### 4. Frontend

- `AppShell.tsx`: hapus toggle IDR/USD di Topbar, `BaseCurrency`, `CurrencyContext`, `useCurrency`, state localStorage `pt-base`.
- Dashboard: card P&L menampilkan total + dua sub-angka (harga / FX).
- HoldingsPage: sub-line FX P&L per row untuk aset non-IDR; "—" untuk aset IDR.
- PerformancePage: card current price-vs-FX P&L; chart ter-split hanya untuk range yang punya data snapshot baru, fallback ke tampilan lama untuk range tanpa data.

## Error handling

- Transaksi dengan `fx_to_idr` 0/kosong: fallback lookup tabel `fx_rate` berdasarkan tanggal transaksi. Kalau itu pun tidak ada: **flag eksplisit di response** (mis. `fx_incomplete: true` di posisi terkait) — bukan silent fallback ke kurs sekarang — supaya data bolong terlihat.
- Semua aritmetika pakai `rust_decimal` seperti existing; tidak ada float.

## Testing

- Unit test engine: FIFO multi-lot dengan kurs beli berbeda-beda, partial sell, full sell, aset IDR (FX = 0), invariant `price + fx = total` untuk realized & unrealized, lot dengan fee.
- Service test: agregasi summary, holdings response, snapshot menulis kolom baru.
- Frontend: tampilan breakdown + fallback "—"/chart lama.

## Di luar scope

- Rekonstruksi dekomposisi untuk snapshot historis lama.
- Re-base seluruh valuasi ke IDR (pendekatan C — ditolak, overkill).
- Perubahan toggle lokal di PerformancePage (tetap ada).
