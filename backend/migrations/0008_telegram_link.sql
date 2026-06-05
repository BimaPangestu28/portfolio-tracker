-- Single-row table: which Telegram chat is linked as the owner.
-- id is CHECKed to 1 so the app can only ever have one link (single-user app).
CREATE TABLE telegram_link (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  chat_id INTEGER NOT NULL,
  username TEXT,
  linked_at TEXT NOT NULL
);
