-- CS chatbot (Phase 1): isolated customer-service tables. Fully separate from the
-- owner-only chat_message table — the CS agent must never touch owner data.

CREATE TABLE cs_conversation (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  channel       TEXT NOT NULL DEFAULT 'web' CHECK (channel IN ('web', 'whatsapp')),
  visitor_name  TEXT,
  visitor_email TEXT,
  visitor_phone TEXT,
  session_token TEXT NOT NULL UNIQUE,
  status        TEXT NOT NULL DEFAULT 'bot'
    CHECK (status IN ('bot', 'needs_human', 'resolved')),
  created_at    TEXT NOT NULL,
  last_msg_at   TEXT NOT NULL
);
CREATE INDEX idx_cs_conversation_token  ON cs_conversation (session_token);
CREATE INDEX idx_cs_conversation_status ON cs_conversation (status, id);

CREATE TABLE cs_message (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  conversation_id INTEGER NOT NULL REFERENCES cs_conversation (id) ON DELETE CASCADE,
  role            TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
  content         TEXT NOT NULL,
  created_at      TEXT NOT NULL
);
CREATE INDEX idx_cs_message_conv ON cs_message (conversation_id, id);

CREATE TABLE cs_kb_doc (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  title      TEXT NOT NULL,
  source     TEXT,
  body       TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE cs_kb_chunk (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  doc_id     INTEGER NOT NULL REFERENCES cs_kb_doc (id) ON DELETE CASCADE,
  text       TEXT NOT NULL,
  embedding  BLOB,                 -- little-endian f32 vector; NULL until embedded
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_cs_kb_chunk_doc ON cs_kb_chunk (doc_id);

CREATE TABLE cs_product (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  name        TEXT NOT NULL,
  description TEXT,
  price       REAL,
  currency    TEXT,
  availability TEXT,
  active      INTEGER NOT NULL DEFAULT 1,
  updated_at  TEXT NOT NULL
);

CREATE TABLE cs_order (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
  external_ref     TEXT NOT NULL UNIQUE,
  customer_name    TEXT,
  customer_contact TEXT,
  status           TEXT NOT NULL,
  details_json     TEXT,
  updated_at       TEXT NOT NULL
);

CREATE TABLE cs_escalation (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  conversation_id INTEGER NOT NULL REFERENCES cs_conversation (id) ON DELETE CASCADE,
  reason          TEXT NOT NULL,
  summary         TEXT NOT NULL,
  status          TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'handled')),
  created_at      TEXT NOT NULL,
  handled_at      TEXT
);
CREATE INDEX idx_cs_escalation_open ON cs_escalation (status, id);
