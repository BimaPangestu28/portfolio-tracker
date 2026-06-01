# Investment Tracker — Phase 4 (Chatbot, 2 channels) Design Spec

**Tanggal:** 2026-06-01
**Status:** Disetujui untuk dieksekusi (mandate "lanjut sampai selesai" — default + ASUMSI ditandai)
**Depends on:** Fase 1 (portfolio summary), Fase 3A (`llm::claude` client).

---

## 1. Problem & scope

Chatbot LLM (Claude) yang bisa **tanya-jawab tentang portfolio** lewat dua channel: **chat panel in-app**
(dashboard) dan **WhatsApp**. Menyatukan ide WhatsApp bot dari awal.

### ASUMSI / keputusan default (koreksi kalau salah)
- **A1. Q&A read-only dulu** (MVP). Chatbot menjawab pertanyaan ("net worth gue berapa", "alokasi crypto
  berapa persen", "transaksi terakhir apa") dengan **context injection**: backend mengambil snapshot
  portfolio (summary + holdings + transaksi/cashflow terakhir) dan menyuntikkannya sebagai konteks ke satu
  panggilan Claude — bukan loop tool-use multi-langkah. Sederhana, deterministik, gampang dites (mock Claude).
- **A2. Write via chat (mis. "catat beli 0.1 BTC") DITUNDA** ke follow-up; bila ditambah, harus lewat
  review_item (prinsip "LLM nggak auto-commit"). Tidak di MVP.
- **A3. In-app channel fully tested; WhatsApp adapter = inbound parser (Meta Cloud API webhook shape)
  + core bersama; outbound send (Graph API) butuh kredensial → live DITUNDA**, didesain mockable. Verify
  token webhook via env `WHATSAPP_VERIFY_TOKEN`.
- **A4. Satu percakapan** (single-user): histori disimpan di tabel `chat_message`. Tidak ada multi-session.
- **A5. Model** reuse `llm::claude` (Claude Messages API) dari Fase 3A; chat memakai system prompt + pesan
  user berisi pertanyaan + konteks portfolio yang disuntik. Model default sama (`claude-sonnet-4-6`,
  override `INGEST_MODEL`/atau `CHAT_MODEL`).

### Out of scope
Tool-use loop multi-langkah · write/commit via chat · multi-user/multi-session · WhatsApp outbound live
send · streaming · voice/media chat.

---

## 2. Komponen (boundary)

- **`chat_message` tabel + repo:** `{id, role(user|assistant), content, channel(inapp|whatsapp), created_at}`.
- **`service/chat.rs`:**
  - `build_context(summary, holdings, recent) -> String` — **pure**: rangkai snapshot portfolio jadi teks
    ringkas (net worth IDR/USD, top holdings, alokasi vs target, beberapa transaksi terakhir). Unit-tested.
  - `answer(db, claude, channel, user_msg) -> String` — simpan pesan user, ambil snapshot (reuse
    `service::portfolio::build_summary` + repos), build_context, panggil `claude.complete(system, [text])`,
    simpan + kembalikan jawaban. Bila `ANTHROPIC_API_KEY` tak ada → error 503 jelas (no panic).
- **WhatsApp adapter** (`api/whatsapp.rs`):
  - `parse_inbound(payload) -> Option<{from, text}>` — **pure** parser bentuk Meta Cloud API webhook
    (`entry[].changes[].value.messages[].text.body`). Unit-tested dgn sample payload.
  - `GET /chat/whatsapp/webhook` — verifikasi (`hub.mode=subscribe`, `hub.verify_token`==env,
    echo `hub.challenge`). `POST /chat/whatsapp/webhook` — parse_inbound → `chat::answer(channel=whatsapp)`;
    outbound send Graph API = fungsi `send_whatsapp` yang **di-stub/log bila kredensial absen** (live ditunda).
- **In-app API:** `POST /chat {message}` → `{reply}`; `GET /chat/history` → `[chat_message]`.
- **Frontend Chat page:** panel pesan (bubble user/assistant) + input; kirim ke `POST /chat`, render histori.

---

## 3. Data model
`0006_chat.sql`:
```sql
CREATE TABLE chat_message (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  role TEXT NOT NULL,        -- 'user' | 'assistant'
  content TEXT NOT NULL,
  channel TEXT NOT NULL,     -- 'inapp' | 'whatsapp'
  created_at TEXT NOT NULL
);
```

## 4. Logika inti (pure, TDD)
- `build_context(...)` — snapshot → teks ringkas (deterministik; uji format & angka).
- `whatsapp::parse_inbound(payload)` — sample Meta webhook → `(from, text)`; payload non-pesan (status
  update) → `None`.

## 5. Error handling & testing
- LLM gagal/no-key → `503` jelas; histori user tetap tersimpan (atau di-rollback — pilih: simpan user msg,
  lalu bila LLM gagal kembalikan error tanpa menyimpan assistant msg). No panic; secret tak di-log.
- WhatsApp webhook verify token salah → `403`. Payload tak dikenal → `200` no-op (jangan error ke Meta).
- Tests: `build_context` (pure), `parse_inbound` (pure + non-message None), chat repo, `answer` dengan mock
  Claude (in-memory DB), in-app API; frontend Chat page (MSW). Live Claude `#[ignore]`.

## 6. Pemecahan plan
- **4-backend:** migration chat_message + repo; `service/chat.rs` (build_context + answer); in-app API
  `/chat` + `/chat/history`; WhatsApp adapter (parse_inbound + GET/POST webhook + stubbed send).
- **4-frontend:** Chat page (panel + input) + schemas/hooks + nav.

## 7. Risiko
- Konteks injeksi membatasi kedalaman jawaban (tanpa tool-use, agent hanya tahu yang disuntik) — cukup
  untuk Q&A umum; pertanyaan sangat spesifik bisa meleset → bisa ditingkatkan ke tool-use nanti.
- WhatsApp live perlu nomor bisnis + token Meta + webhook publik (ngrok/host) → di luar MVP (A3).
- Biaya per pesan (Claude) — satu panggilan per pertanyaan, konteks ringkas.
