-- Phase 3: agenda events. start_at is TEXT UTC Z-format (lexicographic ==
-- chronological, same as reminders.remind_at); created_at is audit RFC3339.
CREATE TABLE events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT NOT NULL,
  location TEXT,
  notes TEXT,
  start_at TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'scheduled'
    CHECK (status IN ('scheduled', 'cancelled')),
  created_at TEXT NOT NULL
);

-- Pre-event reminders are materialized reminder rows linked to their event.
ALTER TABLE reminders ADD COLUMN event_id INTEGER REFERENCES events(id);

CREATE INDEX idx_events_schedule ON events (status, start_at);
