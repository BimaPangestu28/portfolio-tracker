# Noah — Kelola Transaksi via Chat

**Status:** Disetujui (desain) — 2026-06-18
**Area:** `backend/src/assistant` (tools + dispatcher), `backend/src/ingestion`, `backend/src/repo`, `backend/src/service`

## Ringkasan

Assistant Noah saat ini hanya bisa mencatat transaksi investasi lewat alur review
foto/PDF; tidak ada cara membuat, mengedit, atau menghapus transaksi dari chat.
Akibatnya transaksi reksadana tercatat salah: pembelian yang hanya menyertakan
nominal rupiah jatuh ke fallback `quantity = nominal, price = 1`, sehingga jumlah
unit dan valuasi posisi meleset (≈2000×) dan tidak ada jalan koreksi selain Web UI.

Keempat keluhan pengguna berakar pada satu rangkaian: transaksi reksadana butuh
tiga angka — NAV, unit, total — tetapi sistem hanya menangkap nominal, dan tidak
ada jalur koreksi dari chat. Solusinya: bisa **meng-input** angka yang benar, bisa
**menangkap** NAV+unit dari foto detail, dan bisa **mengoreksi** baris yang
terlanjur salah.

## Tujuan

1. Input transaksi manual lewat chat (langsung buat + konfirmasi satu langkah).
2. OCR menangkap NAV dan jumlah unit saat dokumen menampilkannya (mis. layar detail
   transaksi yang sudah settle), bukan hanya nominal total.
3. Edit dan hapus transaksi yang sudah terkonfirmasi lewat chat.
4. Perbaiki bug qty reksadana: catat `quantity = unit`, `price = NAV` — bukan
   `quantity = nominal, price = 1`.

## Non-tujuan (sengaja di luar scope)

- Membuat instrumen baru lewat chat (tetap lewat Web UI). Instrumen pada kasus
  pengguna — "Majoris Pasar Uang Indonesia" — sudah terdaftar.
- Menghapus akun lewat chat.
- Backfill NAV historis otomatis untuk pembelian lama.

## Keputusan desain

- **Input manual:** langsung buat transaksi dengan konfirmasi dua langkah (mengikuti
  pola `create_account`), bukan lewat pending review.
- **Koreksi:** edit penuh atas field transaksi yang ada, plus hapus.
- **Fallback NAV:** kalau reksadana hanya punya nominal dan NAV/unit tidak diketahui,
  **bot bertanya** ke pengguna — tidak diam-diam mencatat `price = 1`.
- **Pengecualian fallback `price = 1`:** dipertahankan **hanya** untuk pembelian baru
  via OCR yang memang hanya menampilkan rupiah (NAV-nya T+1, belum tersedia saat
  konfirmasi). Ini bukan bug; itu satu-satunya data yang ada saat itu. Jalur manual
  tidak memakai fallback ini.

## Arsitektur & komponen

### 1. Resolver bersama `(quantity, price)` — `backend/src/service/txn_entry.rs` (baru)

Mengangkat logika yang sekarang tersebar di `ingestion/review.rs`
(`amount_only_qty_price` + cabang buy/sell di `confirm`) menjadi satu unit yang
dipakai baik oleh jalur manual-create maupun confirm-OCR.

**Antarmuka:**

```
struct EntryInput {
    account_id: i64,
    instrument_id: i64,
    entry_type: String,        // buy|sell|dividend|interest|fee|deposit|withdrawal|opening_balance
    executed_at: String,       // dikoersi via to_rfc3339()
    quantity: Option<String>,
    price_native: Option<String>,
    amount_native: Option<String>,
    fee_native: Option<String>,
    currency: String,
    note: Option<String>,
}

enum ResolveError {
    NeedNavOrUnits,            // reksadana, hanya nominal, NAV & unit tak diketahui
    Other(anyhow::Error),
}

// Mengembalikan (quantity, price_native) ter-resolve, atau NeedNavOrUnits.
fn resolve_qty_price(db, ins: &InstrumentRow, input, allow_price_one_fallback: bool,
                     note: &mut Option<String>) -> Result<(String, String), ResolveError>
```

**Aturan resolusi:**

| Punya | Hasil |
|-------|-------|
| `quantity` + `price_native` | pakai langsung |
| `amount` + `price_native` (NAV) | `quantity = amount / price` (4 dp untuk bibit) |
| `amount` + `quantity` | `price = amount / quantity` |
| reksadana, hanya `amount`, ada NAV tersimpan | seperti sekarang: `quantity = amount/NAV`, `price = NAV` |
| reksadana, hanya `amount`, NAV/unit tak ada, `allow_price_one_fallback=false` | `Err(NeedNavOrUnits)` → bot bertanya (jalur manual) |
| reksadana, hanya `amount`, NAV/unit tak ada, `allow_price_one_fallback=true` | `quantity = amount`, `price = 1` (jalur OCR beli-baru) |

**Convention-lock dipertahankan:** kalau instrumen sudah punya baris `price = 1`
(`has_price_one_txn`), tetap di konvensi nominal agar posisi tidak tercampur — sesuai
komentar `review.rs:106-113`. Mengedit baris lama ke unit asli "membuka" derivasi NAV.

`ingestion/review.rs::confirm()` di-refactor memanggil resolver ini dengan
`allow_price_one_fallback = true` sehingga perilaku jalur OCR tidak berubah.

### 2. Empat tool assistant — `backend/src/assistant/tools.rs` + `dispatcher.rs`

Mengikuti pola dua langkah `create_account` (panggilan tanpa `confirm` mengembalikan
ringkasan untuk dikonfirmasi; dengan `confirm: true` mengeksekusi).

- **`create_transaction`** — param: `account` (nama/id), `instrument` (nama/simbol/id),
  `entry_type`, `executed_at`, salah satu dari (`quantity`+`price_native`) atau
  `amount_native`, `fee_native?`, `currency?` (default IDR), `note?`, `confirm?`.
  Tanpa `confirm`: balas ringkasan, mis. *"Mau saya catat: BELI Majoris Pasar Uang
  Indonesia 1.236,7898 unit @ NAV Rp1.617,0896 = Rp2.000.000 di akun Bibit #4.
  Konfirmasi?"*. Dengan `confirm`: jalankan resolver (`allow_price_one_fallback =
  false`) lalu `transactions::create`. Bila `NeedNavOrUnits`: balas minta NAV atau
  jumlah unit.
- **`list_transactions`** — param: `instrument?`, `account?`, `limit?` (default 10).
  Balas id + ringkasan (tanggal, tipe, instrumen, `qty @ price`, total). Diperlukan
  untuk menemukan id sebelum edit/hapus.
- **`edit_transaction`** — param: `id` + field yang diubah (`entry_type?`,
  `executed_at?`, `quantity?`, `price_native?`, `amount_native?`, `fee_native?`,
  `account?`, `instrument?`, `note?`), `confirm?`. Dua langkah: ringkas perubahan,
  lalu eksekusi. Inilah yang langsung memperbaiki baris Majoris `price = 1` ke unit
  asli.
- **`delete_transaction`** — param: `id`, `confirm?`. Dua langkah. Memakai
  `transactions::delete` yang sudah menangani FK ke `review_item`.

### 3. Tambahan repo — `backend/src/repo/transactions.rs`

- `list_recent(db, limit, instrument_id: Option<i64>, account_id: Option<i64>) ->
  Vec<Transaction>` — urut `executed_at DESC`.
- `update(db, id, patch) -> Transaction` — `UPDATE txn SET ...`, memakai ulang
  normalisasi fx IDR yang sama seperti `create` (fx_to_idr = 1 untuk IDR, fx_to_usd
  diturunkan dari rate terbaru). Validasi semua desimal sebelum menulis.
- `delete` sudah ada.

### 4. Perubahan prompt OCR — `backend/src/ingestion/ingest.rs` (`SYSTEM_PROMPT`)

Tambah aturan untuk reksadana:
> Jika dokumen adalah tampilan detail/settled yang menampilkan "NAV" dan "Jumlah
> Unit", isi `price_native = NAV` dan `quantity = unit`. Hanya gunakan
> `amount_native` saja bila unit/NAV TIDAK ditampilkan (mis. order beli yang masih
> pending). Jangan pernah mengarang nilai.

Ini membuat screenshot detail transaksi (yang memuat NAV + unit) ter-capture benar,
sementara layar beli (hanya rupiah) tetap memakai jalur amount-only.

### 5. Resolusi instrumen & akun untuk input manual

Memakai ulang matcher yang ada (`suggest_instrument_for_entry`, `resolve_account`)
agar pengguna bisa menyebut nama/simbol. Bila instrumen tidak ditemukan: tool
mengarahkan pengguna menambah instrumen lewat Web UI (pembuatan instrumen di luar
scope). Akun memakai `list_accounts`/`resolve_account` yang sama.

## Aliran data

```
Chat (teks)  ── create_transaction (tanpa confirm) ─→ ringkasan ─→ user "iya"
             ── create_transaction (confirm) ─→ resolve_qty_price ─→ transactions::create
Foto/PDF     ── vision extract (NAV+unit bila ada) ─→ review_item ─→ confirm_review
             ── confirm() ─→ resolve_qty_price(allow_price_one_fallback=true) ─→ create
Koreksi      ── list_transactions ─→ id ─→ edit_transaction / delete_transaction (confirm)
```

## Penanganan error

- Instrumen/akun tak dikenal → pesan minta perjelas atau arahkan ke Web UI.
- `entry_type` tak valid → ditolak `TxnType::from_str` (sudah ada).
- Reksadana hanya nominal tanpa NAV/unit (jalur manual) → `NeedNavOrUnits` → bot
  minta NAV atau unit.
- Tanggal tak terbaca → pakai `to_rfc3339`; bila gagal, minta format yang benar.
- Semua field desimal divalidasi sebelum persist (sudah ada di `transactions::create`;
  ditambahkan untuk `update`).

## Testing

- **Resolver** (`service/txn_entry.rs`): unit + NAV; nominal + NAV; nominal + unit;
  reksadana nominal-saja tanpa NAV → `NeedNavOrUnits` (manual) vs fallback `price=1`
  (OCR); convention-lock saat sudah ada baris `price=1`.
- **Repo**: `update` round-trip + normalisasi fx IDR; `list_recent` urutan & filter.
- **Dispatcher**: gating konfirmasi dua langkah untuk create/edit/delete (tanpa
  `confirm` tidak menulis apa pun).
- Test lama `amount_only_buy_without_nav_falls_back_to_price_one` tetap valid (jalur
  OCR tidak berubah).

## Rencana fase (untuk tahap plan)

- **Fase 1:** resolver bersama + `create_transaction` + `list_transactions` + prompt
  OCR — agar data yang benar bisa masuk.
- **Fase 2:** `edit_transaction` + `delete_transaction` + `transactions::update` —
  koreksi data lama (langsung memperbaiki baris Majoris).
