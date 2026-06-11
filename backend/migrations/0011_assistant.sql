-- Personal assistant phase 1: todos and reminders.
-- Columns compared or sorted in SQL (due_at, remind_at) are TEXT, UTC,
-- second precision with trailing Z ("2026-06-12T02:00:00Z") so lexicographic
-- order is chronological order. Audit columns (created_at, completed_at,
-- sent_at) are RFC3339 UTC like the rest of the schema.
CREATE TABLE todos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT NOT NULL,
  notes TEXT,
  due_at TEXT,
  status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'done')),
  created_at TEXT NOT NULL,
  completed_at TEXT
);

CREATE TABLE reminders (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  todo_id INTEGER REFERENCES todos(id),
  message TEXT NOT NULL,
  remind_at TEXT NOT NULL,
  recurrence TEXT NOT NULL DEFAULT 'none'
    CHECK (recurrence IN ('none', 'daily', 'weekly', 'monthly')),
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'sent', 'cancelled')),
  sent_at TEXT
);

CREATE INDEX idx_reminders_due ON reminders (status, remind_at);
