CREATE TABLE connector (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id INTEGER NOT NULL REFERENCES account(id),
  kind TEXT NOT NULL,
  label TEXT NOT NULL,
  config_json TEXT NOT NULL,
  cursor TEXT,
  last_synced_at TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL
);
