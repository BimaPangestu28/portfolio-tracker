# Customer-Service Chatbot — Design (Phase 1: Web Widget)

**Date:** 2026-06-16
**Status:** Approved design, pending implementation plan
**Scope:** Single-tenant (owner's own customers). Phase 1 = embeddable web widget.
Phase 2 (WhatsApp CS via a second gateway) is intentionally out of this spec but the
agent core is built channel-agnostic so it can be added without rework.

## 1. Goal

A public, embeddable customer-service chatbot the owner can drop onto any of their
websites via a `<script>` tag. It answers from a knowledge base, looks up a few kinds
of live data (pricing, order/booking status, optionally Upwork project status), and
escalates to a human (the owner) when it can't help.

It **reuses the existing "pipes"** of the Noah backend — the LLM client
(`llm/claude.rs`), the agentic tool-loop pattern (`assistant/agent.rs`), the chat
storage pattern (`repo/chat.rs`), and the channel plumbing — but runs as a **separate,
isolated `cs` module** with its own persona and a **narrow, read-only toolset**. The CS
agent has **no access** to the owner's private Noah tools (portfolio, invoices, ClickUp,
todos, calendar). This isolation is the central security property of the design.

## 2. Non-goals (Phase 1)

- WhatsApp CS channel (Phase 2: second gateway instance + per-contact routing +
  WhatsApp proactive-send for owner follow-up).
- Live human takeover / real-time two-way relay. Escalation is **async**: the bot
  captures the question + contact, notifies the owner, owner follows up out-of-band.
- Multi-tenant / reselling to other businesses.

## 3. Key decisions (from brainstorming)

| Decision | Choice |
|---|---|
| Tenancy | Single-tenant (owner only) |
| Reuse strategy | Reuse pipes, new isolated CS persona + read-only tools |
| Knowledge | KB (provided docs) + live lookups + human escalation |
| KB retrieval | **Semantic embeddings** from the start |
| Embedding provider | OpenAI `text-embedding-3-small`, reusing the existing `OPENAI_API_KEY` |
| Vector store | Embeddings stored as BLOB in SQLite; brute-force cosine in Rust (in-memory cache). Swap to `sqlite-vec` later if KB grows large. |
| Escalation model | Async (capture + notify, owner follows up later) |
| Escalation targets | Telegram + in-app inbox |
| Web identity | Pre-chat form: capture name + contact **before** chat |
| Order/pricing data | New `cs_*` SQLite tables, owner-populated |
| WhatsApp number | Separate dedicated CS number (Phase 2) |

## 4. Architecture

```
Third-party website ──<script data-key>──▶ cs-widget.js (separate bundle, Shadow DOM)
                                                │  pre-chat form (name + contact)
                                                ▼
                          POST /public/cs/session ──┐  PUBLIC TIER:
                          POST /public/cs/message ──┤  CORS allowlist + site-key + rate-limit
                          GET  /public/cs/history ──┘
                                                │
                                          cs::agent (tool loop)
                            CS persona + read-only tools + semantic KB
                                                │
   ┌──────────────┬───────────────┬────────────────┬───────────────────┐
 kb_search     get_pricing     lookup_order    get_project_status   escalate_to_human
 (embeddings)  (cs_product)    (cs_order)      (Upwork, guarded)    (Telegram + inbox)
                                                │
                                    cs_* tables (SQLite, isolated)

Admin (inside Noah SPA, JWT-protected): manage KB / pricing / orders + CS Inbox.
```

## 5. Backend components (new `cs/` module)

All new code lives in an isolated subtree so the layering stays clean and the CS agent
can never reach Noah's tools.

- `cs/agent.rs` — the CS tool loop. Borrows the LLM client and the loop *pattern* from
  `assistant/agent.rs`, but has its own system prompt and its own tool registry. Where
  practical, extract the channel-agnostic loop mechanics into a small shared helper
  rather than copy-pasting; if extraction is too invasive, a focused parallel
  implementation is acceptable (keep it small).
- `cs/tools.rs` + `cs/dispatcher.rs` — definitions and routing for the read-only tools
  in §7. The dispatcher only knows about CS tools.
- `cs/kb.rs` — document chunking, embedding (OpenAI), vector storage, and cosine
  retrieval. Maintains an in-memory cache of `(chunk_id, vector)` refreshed on KB
  changes; falls back to keyword/FTS search if embedding is unavailable.
- `cs/escalation.rs` — create a `cs_escalation` row, notify the owner via Telegram
  (reuse `TelegramClient.send_message`), and surface it in the in-app inbox.
- `repo/cs.rs` — all SQL for the `cs_*` tables (runtime `sqlx::query`/`query_as`, per
  repo convention).
- `api/cs_public.rs` — public endpoints (session / message / history).
- `api/cs_admin.rs` — JWT-protected admin endpoints (KB/pricing/orders CRUD + inbox).

## 6. Data model (migrations `backend/migrations/00NN_cs_*.sql`)

```sql
cs_conversation(
  id INTEGER PK,
  channel TEXT NOT NULL,            -- 'web' (Phase 2 adds 'whatsapp')
  visitor_name TEXT,
  visitor_email TEXT,
  visitor_phone TEXT,
  session_token TEXT NOT NULL UNIQUE,
  status TEXT NOT NULL,             -- 'bot' | 'needs_human' | 'resolved'
  created_at TEXT NOT NULL,
  last_msg_at TEXT NOT NULL
)

cs_message(
  id INTEGER PK,
  conversation_id INTEGER NOT NULL REFERENCES cs_conversation(id),
  role TEXT NOT NULL,              -- 'user' | 'assistant' | 'system'
  content TEXT NOT NULL,
  created_at TEXT NOT NULL
)
-- Deliberately separate from chat_message (owner-only history).

cs_kb_doc(
  id INTEGER PK,
  title TEXT NOT NULL,
  source TEXT,                     -- e.g. 'faq', 'manual', filename
  body TEXT NOT NULL,
  updated_at TEXT NOT NULL
)

cs_kb_chunk(
  id INTEGER PK,
  doc_id INTEGER NOT NULL REFERENCES cs_kb_doc(id) ON DELETE CASCADE,
  text TEXT NOT NULL,
  embedding BLOB,                  -- f32 vector, little-endian; NULL until embedded
  updated_at TEXT NOT NULL
)

cs_product(
  id INTEGER PK,
  name TEXT NOT NULL,
  description TEXT,
  price REAL,
  currency TEXT,
  availability TEXT,
  active INTEGER NOT NULL DEFAULT 1,
  updated_at TEXT NOT NULL
)

cs_order(
  id INTEGER PK,
  external_ref TEXT NOT NULL UNIQUE,   -- the ref a customer quotes
  customer_name TEXT,
  customer_contact TEXT,               -- email/phone used to verify lookups
  status TEXT NOT NULL,
  details_json TEXT,
  updated_at TEXT NOT NULL
)

cs_escalation(
  id INTEGER PK,
  conversation_id INTEGER NOT NULL REFERENCES cs_conversation(id),
  reason TEXT NOT NULL,
  summary TEXT NOT NULL,
  status TEXT NOT NULL,            -- 'open' | 'handled'
  created_at TEXT NOT NULL,
  handled_at TEXT
)
```

Indexes: `cs_message(conversation_id, created_at)`, `cs_conversation(session_token)`,
`cs_conversation(status)`, `cs_order(external_ref)`, `cs_escalation(status)`.

## 7. Toolset (narrow, read-only)

- `kb_search(query)` → top-k relevant KB chunks via semantic search.
- `get_pricing(query?)` → active packages/prices from `cs_product`.
- `lookup_order(ref, contact)` → order/booking status. **Anti-enumeration guard:**
  requires both `ref` AND a matching `contact`; returns only coarse status, never
  sensitive internals.
- `get_project_status(ref, contact)` → Upwork project status, **tightly guarded**:
  only when `ref` + `contact` match; returns only coarse status ("in progress" /
  "delivered"); **never** contract value or financials. (May be deferred to Phase 1.5
  if verification design needs more thought.)
- `escalate_to_human(reason, summary)` → creates `cs_escalation`, sets conversation
  `status='needs_human'`, notifies Telegram + inbox.

The CS dispatcher exposes **only** these tools. No Noah tool is reachable.

## 8. Public auth & abuse protection

- **CORS allowlist** from `CS_ALLOWED_ORIGINS` (owner's domains) — the real gate.
  Replaces `CorsLayer::permissive()` for the public group only; the rest of the app
  keeps its current CORS.
- **Site-key** (`data-key` on the script) identifies/routes the widget. It is **not a
  secret** (it ships in public JS), so it is not relied on for security.
- **Session token** — opaque, random, stored on `cs_conversation`, issued by
  `/public/cs/session`. Required on subsequent `/message` and `/history` calls.
- **Rate limiting** (tower-governor or equivalent) per-IP and per-session; plus caps on
  input length and number of messages per conversation.
- The `cs` module holds no handle to the protected/gateway tiers.

## 9. Web widget (frontend)

- A **separate Vite entry** builds `cs-widget.js` (+ CSS) as a lightweight bundle
  (vanilla or Preact), rendered in a **Shadow DOM** so host-site CSS can't bleed in.
  It does not pull in the main SPA's React Query / auth / PWA machinery.
- Flow: pre-chat form (name + email/phone) → `POST /public/cs/session` → chat panel →
  `POST /public/cs/message`. Optional `GET /public/cs/history` to restore on reload.
- Embed snippet:
  `<script src="https://portfolio.catalystlabs.id/cs-widget.js" data-key="..." defer></script>`
- Served by Caddy at a stable path.

## 10. Admin UI (inside Noah SPA, JWT, existing schemas.ts + hooks.ts pattern)

- **KB manager** — CRUD docs; saving (re)chunks + (re)embeds.
- **Pricing manager** — CRUD `cs_product`.
- **Orders manager** — CRUD `cs_order` (owner populates).
- **CS Inbox** — list conversations, read transcripts, see escalations, mark resolved.

## 11. Persona / prompt

CS tone, default Bahasa Indonesia but follows the customer's language. **Grounded-only:**
answer from KB/tools, never invent facts. Escalation rules: when it can't answer, when
the customer asks for a human, or on sensitive matters. **Never** reveal internal /
owner / system information. Politely declines off-topic requests.

## 12. Error handling

- LLM failure → friendly fallback message + offer escalation.
- Embedding failure → fall back to keyword/FTS retrieval.
- Tool errors → never leak internals to the customer.
- Rate limit → friendly 429.
- Escalation notify failure → non-fatal; the customer still gets a reply, escalation row
  still persists.

## 13. Testing

- Domain: cosine similarity, chunking.
- Repo: `cs_*` queries.
- Tool dispatch routing.
- Agent loop with a mocked LLM.
- Public endpoints: auth/session, CORS, rate-limit.
- **Leak-guard tests:** assert the CS agent cannot reach owner tools/data; prompt-
  injection attempts don't exfiltrate internal info or escalate privileges.
- Widget smoke test.

## 14. New env vars

- `CS_ALLOWED_ORIGINS` — comma-separated allowed origins for the public CORS group.
- `CS_WIDGET_KEY` — site key for the embed script.
- `CS_EMBED_MODEL` — embedding model (default `text-embedding-3-small`), reusing
  `OPENAI_API_KEY` / `INGEST_BASE_URL`.
- Rate-limit knobs (e.g. `CS_RATE_PER_MIN`).

## 15. Phase 2 preview (not in this plan)

WhatsApp CS via a second `whatsapp-gateway` instance (own `AUTH_DIR` + `GATEWAY_TOKEN`),
a `POST /cs/whatsapp/inbound` endpoint routing **per-sender** into `cs_conversation`
(channel='whatsapp'), a second `WaState` for pairing the CS number from the dashboard,
and WhatsApp proactive-send so the owner's follow-up can reach the customer. The Phase 1
agent core is channel-agnostic to make this a plug-in.
