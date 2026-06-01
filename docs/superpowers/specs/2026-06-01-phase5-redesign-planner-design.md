# Investment Tracker — Phase 5 (Redesign + Planner-grade features) Design Spec

**Tanggal:** 2026-06-01
**Status:** Disetujui untuk dieksekusi (mandate; keputusan default ditandai)
**Source desain:** `docs/design/claude-design-source/` (handoff dari Claude Design — `theme.css`,
`app.jsx`, `ui.jsx`, `charts.jsx`, `data.js`, `page_*.jsx`, `chat-transcript.md`). **Recreate
visual-nya seperti di source** (baca file-file itu langsung saat implementasi).

---

## 1. Tujuan

Redesign frontend ke **"calm fintech command center"** (dark default + blue, Inter, tabular-nums,
token HSL ala-shadcn + palet kategorikal) **dan** menambah **fitur planner-grade** yang butuh
backend baru. Berdasarkan bundle Claude Design yang user setujui.

### Keputusan (default — koreksi bila perlu)
- **Visual:** dark default + light toggle, primary blue (`--primary-h: 217`), Inter, token di
  `theme.css` (port apa adanya). Palet kategori per kelas aset konsisten (donut=bar=legend).
- **IA dikonsolidasi 8→6 menu:** Dashboard · **Portofolio** (tab Holdings/Transaksi) · **Rencana**
  (Planner) · Budget · **Data** (tab Sinkron=Connectors / Review=Import) · Chat.
- **Extend backend** untuk metrik planner (user pilih ini): savings rate, dividend TTM + passive
  yield, liquid + emergency runway, konsentrasi top-holding, day-delta portfolio, komposisi
  kekayaan per-waktu, dan **Financial Goals** (CRUD baru). Today's movers per-holding =
  **best-effort** (lihat §4).
- **Auth (login/first-run/lock):** **frontend-only mock** (backend belum punya auth; sesuai
  prototype). Sandi master disimpan lokal (hash di localStorage); ditandai sebagai mock,
  porting ke auth asli = follow-up.
- **Copyright:** "© 2026 catalystlabs.id" (footer sidebar + login).
- **Tweaks panel** (accent/density/radius/theme/currency) = opsional, prioritas rendah.

### Hard constraints
- **Jangan ubah data layer Fase 1–4 yang sudah ada** kecuali untuk MENAMBAH (endpoint/field baru).
  Money tetap string; parse cuma buat display (`src/lib/format.ts`). No `any`; strict TS.
- Recreate visual dari `docs/design/claude-design-source/` (jangan jiplak struktur internal
  prototype kalau nggak pas — match output-nya, pakai React + komponen kita).
- Tiap sub-fase: `cargo test` + `npm test` + `npm run build` hijau; conventional commits; review gate.

---

## 2. Metrik planner: derivasi vs backend baru

Dari `data.js` (referensi). Sumber data yang ada: positions (Fase 1), txns (dividend/interest),
cashflow (Fase 3B), valuation_snapshot dgn `breakdown_json` per-kategori (Fase 1 scheduler).

| Metrik | Sumber | Cara |
|--------|--------|------|
| Net worth + day delta (portfolio) | valuation_snapshot | snapshot terbaru vs sebelumnya |
| Unrealized/Realized P&L, XIRR | sudah ada (`build_summary`) | reuse |
| Savings rate | cashflow bulan ini | net/income × 100 |
| Dividend TTM + passive yield | txns dividend/interest 12 bln | Σ/net worth × 100 |
| Liquid + emergency runway | positions (kategori "kas"/cash) + cashflow expense | liquid / monthly_expense |
| Konsentrasi top-holding | positions | max(mv)/net worth |
| Komposisi kekayaan per-waktu (stacked) | valuation_snapshot.breakdown_json | deret per-kategori |
| **Financial Goals** | **tabel `goal` BARU** | target + current (derivasi/manual) |
| Today's movers (per-holding) | **butuh harga prev-close** | best-effort: simpan `price_quote` harian → day% per instrumen; bila belum ada history, tampilkan movers kosong/by realized day-delta. **Ditandai.** |

**Endpoint baru:** `GET /portfolio/insights` → semua metrik planner agregat (savings_rate,
dividend_ttm, yield_pct, liquid_idr, runway_months, top_holding{symbol,pct}, day_delta_idr/pct,
composition[] timeseries, movers{gainers[],losers[]}). `GET/POST/DELETE /goals`.

## 3. Dekomposisi (tiap = spec ada di sini, plan + eksekusi sendiri)

- **5A — Backend insights + goals.** `service/insights.rs` (pure aggregators: savings_rate,
  yield, runway, concentration, day_delta, composition; unit-tested), `GET /portfolio/insights`;
  `goal` table + repo + `GET/POST/DELETE /goals`; movers best-effort (daily price snapshot table or
  documented gap). No frontend.
- **5B — Frontend foundation.** Port `theme.css` → `src/index.css`/theme; app shell
  (sidebar collapsible + topbar + mobile bottom-nav/sheet) ala design; **router 6-item IA**
  (Portofolio & Data jadi tab pages); light/dark provider; zod schemas + hooks untuk
  `/portfolio/insights` & `/goals`. Pages masih shell/placeholder.
- **5C — Dashboard command-center.** Net worth hero + sparkline, KPI cards, Kesehatan Portofolio,
  Alokasi donut + drift bars, Komposisi (stacked area), Rekomendasi Rebalancing, Tujuan Keuangan,
  Pergerakan Hari Ini — wired ke `/portfolio/summary` + `/portfolio/insights` + `/goals`. Charts
  via Recharts (port `charts.jsx` visuals).
- **5D — Pages.** Portofolio (Holdings/Transaksi tabs), Rencana (Planner), Budget, Data
  (Connectors/Import tabs), Chat — restyle ke sistem baru, wired ke hooks existing.
- **5E — Auth + polish.** Login/first-run/lock (frontend mock), catalystlabs.id copyright,
  empty/skeleton/toast states, optional Tweaks panel.

## 4. Catatan & risiko
- **Today's movers** butuh harga kemarin per instrumen. MVP: tambah tabel/penyimpanan
  `price_quote` historis harian via scheduler; sampai terkumpul, movers bisa kosong (empty state) —
  **tidak menampilkan angka palsu**. Alternatif: day-delta hanya level-portfolio (dari snapshot).
- **Auth mock**: jangan diklaim sebagai keamanan beneran; dokumentasikan porting ke unlock asli.
- **Goals "current"**: derivasi sederhana (mis. dana darurat = nilai kategori kas; FIRE = net worth;
  custom = manual `current`), bukan logika kompleks.
- Recharts sudah dipakai (Fase 1B) — reuse, theme-kan ke token.

## 5. Acceptance (per sub-fase + keseluruhan)
- 6 halaman konsisten di sistem baru, dark+light, mobile+desktop.
- Backend insights/goals ada test (pure aggregator + repo), endpoint smoke OK, no panic.
- Data layer lama utuh; semua test hijau; build bersih; no `any`.
- Dashboard kebaca seperti app fintech beneran (net worth, P&L, kesehatan alokasi, tren, aksi).
