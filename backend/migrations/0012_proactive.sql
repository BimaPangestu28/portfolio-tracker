-- Phase 4: dedup/idempotency log for proactive sends (briefing, recap, alerts).
-- sent_at is audit-only RFC3339 UTC.
CREATE TABLE proactive_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  kind TEXT NOT NULL,
  dedup_key TEXT NOT NULL UNIQUE,
  sent_at TEXT NOT NULL
);
