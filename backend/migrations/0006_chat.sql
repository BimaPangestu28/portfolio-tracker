CREATE TABLE chat_message (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  role TEXT NOT NULL,
  content TEXT NOT NULL,
  channel TEXT NOT NULL,
  created_at TEXT NOT NULL
);
