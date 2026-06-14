-- Fase 4: GTD quick-capture inbox. Raw captures await batch triage.
CREATE TABLE inbox (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  content TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'sorted', 'dropped')),
  created_at TEXT NOT NULL,
  resolved_at TEXT
);

CREATE INDEX idx_inbox_pending ON inbox (status, id);
