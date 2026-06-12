-- Google Calendar sync, phase 1 of N. Single-row integration holding OAuth
-- tokens (encrypted at rest) and the Calendar incremental sync token.
CREATE TABLE google_integration (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  access_token TEXT NOT NULL,          -- AES-GCM ciphertext (base64)
  refresh_token TEXT NOT NULL,         -- AES-GCM ciphertext (base64)
  expiry TEXT NOT NULL,                -- RFC3339 UTC access-token expiry
  scope TEXT NOT NULL,
  calendar_sync_token TEXT,            -- Google events.list nextSyncToken
  status TEXT NOT NULL DEFAULT 'connected'
    CHECK (status IN ('connected', 'disconnected', 'error')),
  last_error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- events ownership + Google linkage. 'local' = app-owned (two-way);
-- 'google' = foreign import (read-only). updated_at drives last-write-wins.
ALTER TABLE events ADD COLUMN source TEXT NOT NULL DEFAULT 'local'
  CHECK (source IN ('local', 'google'));
ALTER TABLE events ADD COLUMN google_event_id TEXT;
ALTER TABLE events ADD COLUMN google_etag TEXT;
ALTER TABLE events ADD COLUMN synced_at TEXT;
ALTER TABLE events ADD COLUMN updated_at TEXT;
UPDATE events SET updated_at = created_at WHERE updated_at IS NULL;

-- UNIQUE index (not an ADD COLUMN ... UNIQUE, which SQLite forbids): a Google
-- event maps to at most one app row, guarding the reconciler against duplicate
-- imports. SQLite treats NULLs as distinct, so unsynced local rows coexist. The
-- index also serves google_event_id lookups.
CREATE UNIQUE INDEX idx_events_google ON events (google_event_id);
CREATE INDEX idx_events_sync ON events (source, synced_at);
