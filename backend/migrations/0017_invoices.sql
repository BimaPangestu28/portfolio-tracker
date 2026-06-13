-- Freelance invoicing: reusable client details + write-once invoices.
CREATE TABLE client (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
  sub_name   TEXT,
  website    TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE invoice (
  id              INTEGER PRIMARY KEY,
  number          TEXT NOT NULL UNIQUE,
  client_id       INTEGER NOT NULL REFERENCES client(id),
  issue_date      TEXT NOT NULL,
  due_date        TEXT NOT NULL,
  subtotal        TEXT NOT NULL,
  total           TEXT NOT NULL,
  line_items_json TEXT NOT NULL,
  created_at      TEXT NOT NULL
);
