CREATE TABLE goal (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  label TEXT NOT NULL,
  note TEXT,
  target_idr TEXT NOT NULL,
  current_kind TEXT NOT NULL,         -- 'cash' | 'networth' | 'manual'
  current_manual_idr TEXT,            -- used when current_kind='manual'
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);
