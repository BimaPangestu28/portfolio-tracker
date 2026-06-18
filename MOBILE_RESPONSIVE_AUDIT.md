# Audit Responsive Mobile — Web Frontend

Stack: React 18 + TypeScript + Vite + Tailwind 3 + custom CSS (`src/index.css`) + Radix UI + Recharts.
Target viewport audit: **320–640px**.

## Ringkasan

Shell/navigasi **sudah responsive** (sidebar → sheet + bottom-nav di `max-width: 880px`, dialog → bottom-sheet di `560px`, auth → 1 kolom di `820px`). Yang bermasalah adalah **konten halaman**.

**Akar masalah berulang:**
1. Inline `style={{ gridTemplateColumns: ... }}` tanpa media query → grid multi-kolom tidak pernah collapse di mobile.
2. Fixed-width Tailwind utilities di form (`w-40`, `w-56`, `w-72`, `w-28`) → field meluber / tidak muat di 320px.
3. Flex row tanpa `flex-wrap` / tanpa stack di mobile → konten saling himpit & truncate berlebihan.
4. Beberapa komponen dengan ukuran fixed (kalender 7 kolom, QR 240px, font hero 30–40px).

Karena polanya seragam, perbaikan paling bersih = **konversi inline-grid → util responsive Tailwind / class CSS dengan breakpoint**, plus penyesuaian font/padding mobile.

---

## P0 — Critical (layout pecah / tidak terpakai di mobile)

| # | File:Line | Masalah | Fix |
|---|-----------|---------|-----|
| 1 | `pages/CsInboxPage.tsx:32` | `gridTemplateColumns: "320px 1fr"` hardcoded → kolom kiri 320px = layar penuh, panel kanan kebuang | Stack vertikal < 768px; list di atas, detail di bawah (atau master-detail toggle) |
| 2 | `components/MonthGrid.tsx:85,92` | Kalender `grid-cols-7` fixed → sel ~40px, angka & dot tak terbaca, touch target kekecilan | Sel `aspect-square` + font/padding mobile; pertimbangkan agenda-list view < 400px |
| 3 | `pages/DashboardPage.tsx:189,247,1212,1228,1260,1278` | 6 grid `gridTemplateColumns` 2-kolom hardcoded, tidak collapse < 880px | Konversi ke `grid-cols-1 md:grid-cols-2` (atau CSS media query) |

## P1 — High (UX buruk di mobile)

| # | File:Line | Masalah | Fix |
|---|-----------|---------|-----|
| 4 | `pages/BudgetPage.tsx:196,226` | `repeat(3,1fr)` & `1.5fr 1fr` fixed → 3 kartu mungil di mobile | `grid-cols-1 sm:grid-cols-2 lg:grid-cols-3` |
| 5 | `pages/SettingsPage.tsx:73,185,240,258` | Form `flex-wrap` dengan `w-40/w-56/w-72/w-28` → total >320px, field meluber | `flex-col` / grid 1-kolom < 600px; ganti fixed width → `w-full sm:w-…` |
| 6 | `pages/InvoicesPage.tsx:53,62,84,90-96` | `minmax(0,1.3fr) minmax(0,1fr)` tidak stack; list item & line-item himpit/overflow | Stack 1-kolom < 768px; line items beri overflow/wrap |
| 7 | `pages/ImportPage.tsx:349,191` | Upload/CSV `1fr 1fr` & field grid `minmax(120px,1fr)` tidak stack | `grid-cols-1 md:grid-cols-2`; field `minmax(0,1fr)` 1-kolom mobile |
| 8 | `pages/PlannerPage.tsx:314` | Form dialog `1fr 1fr` tidak stack | `grid-cols-1 sm:grid-cols-2` |
| 9 | `pages/TugasPage.tsx` — TodoTab:57-80, ReminderTab:43,50-62 | Add-form input+button himpit; datetime input + button overflow 320px; item row truncate berat | `flex-wrap`/stack < 480px; input `w-full`; datetime full-width |
| 10 | `pages/ChatPage.tsx:164,178,50` | Suggestion chips tak word-wrap (overflow); input form sempit; badge header tak muat | `white-space:normal` / `max-width` chip; textarea `w-full`; badge stack di mobile |
| 11 | `components/HyperliquidPositions.tsx:56-123` | 2 tabel 7-kolom; sudah `table-wrap` overflow tapi sempit/teks kecil di 320px | OK via overflow; opsional sembunyikan kolom non-kritis < 480px (Lev/TF) |
| 12 | `pages/CsWhatsAppPage.tsx:55` | QR `size={240}` > sebagian layar 320px | Responsive size (≈160 di < 360px) |

## P2 — Medium / Low (cramped tapi fungsional)

| # | File:Line | Masalah | Fix |
|---|-----------|---------|-----|
| 13 | `pages/ConnectorsPage.tsx:173` / `PlannerPage.tsx:209` | Card grid `minmax(300px/280px,1fr)` ketat < 380px | Turunkan minmax atau `grid-cols-1` < 360px |
| 14 | `index.css:829` (LoginPage `.auth-tag`) | Font hero 30px kebesaran < 480px | Kecilkan ke ~20–22px di mobile |
| 15 | `index.css` `.t-display` (Dashboard hero) | Font 40px kebesaran di mobile | clamp()/breakpoint |
| 16 | `components/HyperliquidCard.tsx:43` | Chart `minTickGap=28` sembunyikan label x < 360px | Naikkan tick gap di mobile |
| 17 | `pages/NewsPage.tsx:26` | Gambar `h-40` (160px) kegedean | `h-28 sm:h-40` |
| 18 | `components/DayEventsPanel.tsx:36` | Item event tak wrap di 320px | beri wrap / stack |
| 19 | `index.css` `.card` padding 20px | Bisa dikecilkan di mobile (16px) | media query padding |

**Sudah aman (tidak perlu diutak-atik):** AppShell nav, dialog bottom-sheet (`index.css:691`), Recharts `ResponsiveContainer`, `.table-wrap` overflow, PerformancePage/AgendaPage container grid (`sm:`/`md:`), NetWorthCard, DataPage tabs, CsDocsPage, CsPricing/CsOrders dialog, NewsQuiz, InboxTab.

---

## Rencana eksekusi (urutan PR)

1. **Fondasi** — tambah util/class responsive di `index.css` (mis. `.grid-2` yang auto-collapse < 768px, `.form-row` stack, mobile padding/font) agar fix konsisten & minim duplikasi.
2. **P0** — Dashboard, CsInbox, MonthGrid (paling kelihatan rusak).
3. **P1** — Budget, Settings, Invoices, Import, Planner, Tugas, Chat, Hyperliquid, WhatsApp QR.
4. **P2** — polish font/padding/gambar/chart.
5. **Verifikasi** — cek tiap halaman di 320 / 375 / 414 / 640 / 768px (DevTools / build).

Estimasi: P0+P1 ~ inti pekerjaan; P2 polish cepat.
