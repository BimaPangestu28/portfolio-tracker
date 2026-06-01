# Investment Tracker — Phase 2 (Auto-sync Connectors) Design Spec

**Tanggal:** 2026-06-01
**Status:** Disetujui untuk dieksekusi (mandate "lanjut sampai selesai" — default + ASUMSI ditandai)
**Depends on:** Fase 1 (ledger/txn, repos), Fase 3A (`review_item` staging untuk unknown).

---

## 1. Problem & scope

Otomasi input untuk sumber yang PUNYA API: **on-chain wallet** (publik, no secret) & **crypto exchange**.
Tarik data → masukin ledger otomatis, idempotent (re-sync nggak dobel).

### ASUMSI / keputusan default (koreksi kalau salah)
- **A1. On-chain EVM dulu (prioritas).** Connector wallet EVM pakai **explorer API ala Etherscan** (`?module=account&action=txlist/tokentx&address=`), input cuma **alamat wallet (publik)** + base URL + opsional API key explorer. Kenapa: paling bernilai buat blockchain dev, **nggak butuh secret sensitif**, deterministik, gampang dites (parser pure atas sample JSON).
- **A2. Exchange (Binance/Indodax) didesain pluggable** lewat trait yang sama, tapi implementasi live (HMAC signing + balance) **ditandai follow-up** karena butuh API key sensitif + signing; Fase 2 kirim **MockConnector** + EVM yang fully tested. Slot exchange siap diisi tanpa ubah framework.
- **A3. Idempotency** via kolom baru `source` + `external_id` di tabel `txn` (nullable) + unique index `(source, external_id)`. Re-sync meng-INSERT hanya yang belum ada.
- **A4. Unknown instrument** (symbol/kontrak yang belum terdaftar) **TIDAK** di-insert diam-diam ke ledger; di-stage ke `review_item` (Fase 3A) `doc_type="connector_sync"`, `needs_attention`. Instrumen yang sudah terdaftar (match by symbol) → masuk ledger langsung dgn dedup. Ini jaga ledger tetap akurat + reuse review queue.
- **A5. Kredensial connector** disimpan di tabel `connector` (`config_json`) untuk single-user self-host. **Enkripsi at-rest ditandai follow-up**; wallet address publik (aman); explorer API key low-sensitivity. Exchange secret (saat diimplementasi) sebaiknya dari env — dicatat.
- **A6. Mapping arah:** transfer masuk wallet → `deposit` instrumen crypto; keluar → `withdrawal`. (Buy/sell harga tidak diketahui dari transfer on-chain → harga kosong, `needs_attention` bila perlu valuasi; user lengkapi.) Konsisten: on-chain memberi kuantitas & arah, bukan harga beli.

### Out of scope
Live exchange signing (follow-up), DeFi LP/staking position decoding, harga historis on-chain, multi-chain selain EVM-compatible, enkripsi secret.

---

## 2. Komponen (boundary)

- **`connector` tabel + repo:** `{id, account_id, kind(evm_wallet|binance|mock), label, config_json, cursor, last_synced_at, enabled, created_at}`.
- **Trait `Connector`** (`src/connectors/mod.rs`): `async fn fetch_new(&self, cursor: Option<&str>) -> Result<SyncBatch, ConnectorError>` → `SyncBatch { txns: Vec<ExternalTxn>, next_cursor: Option<String> }`. `ExternalTxn { external_id, occurred_at(rfc3339), kind, symbol, quantity, fee?, currency }`.
- **EVM connector** (`connectors/evm.rs`): build explorer URL, **pure parser** `parse_txlist(json, address) -> Vec<ExternalTxn>` (native) & `parse_tokentx(json) -> Vec<ExternalTxn>` (ERC-20), tested offline. Network call via reqwest.
- **MockConnector** (`connectors/mock.rs`): returns a fixed `SyncBatch` (for tests/demo + scheduler smoke).
- **Sync service** (`src/service/sync.rs`): given a connector row + a `Connector` impl, fetch_new(cursor), for each ExternalTxn: match instrument by symbol → if known, insert ledger txn (type from kind, qty, price 0/empty, fx defaults, **`source`+`external_id` set**) skipping any `(source, external_id)` already present; if unknown → stage `review_item`. Update `cursor`/`last_synced_at`. Pure helper `dedup_new(existing_ids, batch) -> Vec<ExternalTxn>` unit-tested.
- **Migration `0004_txn_external_id.sql`:** `ALTER TABLE txn ADD COLUMN source TEXT; ALTER TABLE txn ADD COLUMN external_id TEXT;` + `CREATE UNIQUE INDEX idx_txn_source_ext ON txn(source, external_id) WHERE source IS NOT NULL;`. `transactions::create` extended to accept optional `source`/`external_id` (default None → existing behavior unchanged).
- **API:** `GET/POST/DELETE /connectors`, `POST /connectors/:id/sync` → `{ inserted, staged, skipped }`.
- **Scheduler:** extend existing loop to also sync `enabled` connectors (best-effort, errors logged).
- **Frontend:** Connectors page (add EVM wallet: account, label, address, explorer base URL; list with last-synced + "Sync now"; delete). Sync result toast/line.

---

## 3. Data model
- `connector` table (above). Migration `0004` adds `txn.source`/`txn.external_id` + partial unique index.
- `transactions::NewTransaction` gains `source: Option<String>`, `external_id: Option<String>` (default None). Existing callers unaffected (Rust struct update — set None).

## 4. Logika inti (pure, TDD)
- `evm::parse_txlist` / `parse_tokentx` — sample explorer JSON → `ExternalTxn[]` (arah ditentukan dgn membandingkan `to`/`from` ke address; nilai dikonversi dari wei/desimal token ke unit).
- `sync::dedup_new(existing: HashSet<(String,String)>, batch) -> Vec<ExternalTxn>` — buang yang `(source, external_id)` sudah ada.
- `sync::external_to_new_txn(ext, account_id, instrument_id, source) -> NewTransaction` — mapping kind→txn_type, qty, price kosong→"0", fx default.

## 5. Error handling & testing
- Connector fetch gagal → sync endpoint 502, connector lain tetap jalan di scheduler (best-effort, log).
- Unknown instrument → staged (bukan di-drop). Insert dedup → idempotent.
- `transactions::create` validasi decimal tetap berlaku.
- No unwrap/panic produksi; secret tak di-log.
- Tests: EVM parsers (sample JSON), dedup, mapping, connector repo, sync service (pakai MockConnector + DB in-memory), API; frontend Connectors page (MSW). Live EVM test `#[ignore]`.

## 6. Pemecahan plan
- **2-backend:** migration + txn external_id; connector repo; trait + ExternalTxn + Mock; EVM connector + parsers; sync service (dedup/map/insert/stage); API + scheduler hook.
- **2-frontend:** Connectors page + schemas/hooks + nav.

## 7. Risiko
- Format explorer API beragam (Etherscan vs Blockscout) — base URL configurable; parser menargetkan bentuk Etherscan `result[]` standar.
- On-chain tak punya harga → posisi crypto butuh harga terkini (sudah ada via CoinGecko Fase 1) untuk valuasi; cost-basis dari deposit on-chain = 0 (avg cost rendah) → **ASUMSI A6**, user bisa koreksi via edit/opening_balance.
- Exchange live ditunda (A2).
