-- backend/migrations/0015_upwork.sql

-- Single-row Upwork connection (mirrors google_integration). Tokens are stored
-- already-encrypted by the caller (see upwork::crypto). id is always 1.
CREATE TABLE upwork_integration (
  id INTEGER PRIMARY KEY,
  access_token TEXT NOT NULL,
  refresh_token TEXT NOT NULL,
  expiry TEXT NOT NULL,
  scope TEXT NOT NULL,
  earnings_cursor TEXT,
  status TEXT NOT NULL DEFAULT 'connected',
  last_error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- Provenance + idempotency for cashflow rows. Existing/manual rows leave these
-- NULL. SQLite treats NULLs as distinct in a UNIQUE index, so manual entries
-- are unconstrained; only (source, external_ref) pairs from connectors are
-- deduplicated.
ALTER TABLE cashflow ADD COLUMN source TEXT;
ALTER TABLE cashflow ADD COLUMN external_ref TEXT;
CREATE UNIQUE INDEX idx_cashflow_source_ref ON cashflow(source, external_ref);
